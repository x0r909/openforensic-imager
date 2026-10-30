#include "tsk_shim.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tsk/libtsk.h"
#include "tsk/base/tsk_base.h"
#include "tsk/fs/tsk_fs.h"
#include "tsk/img/tsk_img.h"
#include "tsk/vs/tsk_vs.h"

struct ofi_tsk_image {
    TSK_IMG_INFO *image;
};

struct ofi_tsk_vs {
    TSK_VS_INFO *volume_system;
};

struct ofi_tsk_fs {
    TSK_FS_INFO *filesystem;
};

struct ofi_tsk_dir {
    TSK_FS_DIR *directory;
};

struct ofi_tsk_file {
    TSK_FS_FILE *file;
};

static void set_error(char *buffer, size_t size) {
    const char *message = tsk_error_get_errstr();
    uint32_t error_number = tsk_error_get_errno();
    if (buffer == NULL || size == 0) {
        return;
    }
    snprintf(buffer, size, "%s (TSK error %u)",
             message != NULL && message[0] != '\0' ? message : "TSK operation failed",
             error_number);
}

void ofi_tsk_last_error(char *buffer, size_t size) {
    set_error(buffer, size);
}

int ofi_tsk_image_open(const char *path, ofi_tsk_image **output) {
    struct ofi_tsk_image *wrapper;
    if (path == NULL || output == NULL) {
        return -1;
    }
    *output = NULL;
    tsk_error_reset();
    wrapper = (struct ofi_tsk_image *) calloc(1, sizeof(*wrapper));
    if (wrapper == NULL) {
        return -1;
    }
    wrapper->image = tsk_img_open_utf8_sing(path, TSK_IMG_TYPE_DETECT, 0);
    if (wrapper->image == NULL) {
        free(wrapper);
        return -1;
    }
    *output = wrapper;
    return 0;
}

void ofi_tsk_image_close(ofi_tsk_image *image) {
    if (image != NULL) {
        if (image->image != NULL) {
            tsk_img_close(image->image);
        }
        free(image);
    }
}

uint64_t ofi_tsk_image_size(const ofi_tsk_image *image) {
    return image != NULL && image->image != NULL ? (uint64_t) image->image->size : 0;
}

uint32_t ofi_tsk_image_sector_size(const ofi_tsk_image *image) {
    return image != NULL && image->image != NULL ? image->image->sector_size : 0;
}

int ofi_tsk_vs_open(const ofi_tsk_image *image, uint8_t type,
                    ofi_tsk_vs **output) {
    struct ofi_tsk_vs *wrapper;
    if (image == NULL || image->image == NULL || output == NULL) {
        return -1;
    }
    *output = NULL;
    tsk_error_reset();
    wrapper = (struct ofi_tsk_vs *) calloc(1, sizeof(*wrapper));
    if (wrapper == NULL) {
        return -1;
    }
    wrapper->volume_system =
        tsk_vs_open(image->image, 0, (TSK_VS_TYPE_ENUM) type);
    if (wrapper->volume_system == NULL) {
        free(wrapper);
        return -1;
    }
    *output = wrapper;
    return 0;
}

void ofi_tsk_vs_close(ofi_tsk_vs *volume_system) {
    if (volume_system != NULL) {
        if (volume_system->volume_system != NULL) {
            tsk_vs_close(volume_system->volume_system);
        }
        free(volume_system);
    }
}

uint32_t ofi_tsk_vs_type(const ofi_tsk_vs *volume_system) {
    return volume_system != NULL && volume_system->volume_system != NULL
        ? (uint32_t) volume_system->volume_system->vstype : 0;
}

uint32_t ofi_tsk_vs_block_size(const ofi_tsk_vs *volume_system) {
    return volume_system != NULL && volume_system->volume_system != NULL
        ? volume_system->volume_system->block_size : 0;
}

uint32_t ofi_tsk_vs_partition_count(const ofi_tsk_vs *volume_system) {
    return volume_system != NULL && volume_system->volume_system != NULL
        ? volume_system->volume_system->part_count : 0;
}

int ofi_tsk_vs_partition(const ofi_tsk_vs *volume_system, uint32_t index,
                         ofi_partition_info *partition) {
    const TSK_VS_PART_INFO *part;
    uint64_t block_size;
    if (volume_system == NULL || volume_system->volume_system == NULL ||
        partition == NULL) {
        return -1;
    }
    part = tsk_vs_part_get(volume_system->volume_system, index);
    if (part == NULL) {
        return -1;
    }
    block_size = volume_system->volume_system->block_size;
    memset(partition, 0, sizeof(*partition));
    partition->address = part->addr;
    partition->start_sector = part->start;
    partition->sector_count = part->len;
    partition->offset_bytes = part->start * block_size;
    partition->length_bytes = part->len * block_size;
    partition->flags = (uint32_t) part->flags;
    partition->table_num = part->table_num;
    partition->slot_num = part->slot_num;
    if (part->desc != NULL) {
        snprintf(partition->description, sizeof(partition->description), "%s",
                 part->desc);
    }
    return 0;
}

int ofi_tsk_fs_open(const ofi_tsk_image *image, uint64_t offset_bytes,
                    ofi_tsk_fs **output) {
    struct ofi_tsk_fs *wrapper;
    if (image == NULL || image->image == NULL || output == NULL) {
        return -1;
    }
    *output = NULL;
    tsk_error_reset();
    wrapper = (struct ofi_tsk_fs *) calloc(1, sizeof(*wrapper));
    if (wrapper == NULL) {
        return -1;
    }
    wrapper->filesystem = tsk_fs_open_img(image->image, (TSK_OFF_T) offset_bytes,
                                           TSK_FS_TYPE_DETECT);
    if (wrapper->filesystem == NULL) {
        free(wrapper);
        return -1;
    }
    *output = wrapper;
    return 0;
}

void ofi_tsk_fs_close(ofi_tsk_fs *filesystem) {
    if (filesystem != NULL) {
        if (filesystem->filesystem != NULL) {
            tsk_fs_close(filesystem->filesystem);
        }
        free(filesystem);
    }
}

uint32_t ofi_tsk_fs_type(const ofi_tsk_fs *filesystem) {
    return filesystem != NULL && filesystem->filesystem != NULL
        ? (uint32_t) filesystem->filesystem->ftype : 0;
}

uint32_t ofi_tsk_fs_block_size(const ofi_tsk_fs *filesystem) {
    return filesystem != NULL && filesystem->filesystem != NULL
        ? filesystem->filesystem->block_size : 0;
}

uint64_t ofi_tsk_fs_inode_count(const ofi_tsk_fs *filesystem) {
    return filesystem != NULL && filesystem->filesystem != NULL
        ? (uint64_t) filesystem->filesystem->inum_count : 0;
}

uint64_t ofi_tsk_fs_root_inode(const ofi_tsk_fs *filesystem) {
    return filesystem != NULL && filesystem->filesystem != NULL
        ? (uint64_t) filesystem->filesystem->root_inum : 0;
}

int ofi_tsk_dir_open(const ofi_tsk_fs *filesystem, uint64_t inode,
                     ofi_tsk_dir **output) {
    struct ofi_tsk_dir *wrapper;
    if (filesystem == NULL || filesystem->filesystem == NULL || output == NULL) {
        return -1;
    }
    *output = NULL;
    tsk_error_reset();
    wrapper = (struct ofi_tsk_dir *) calloc(1, sizeof(*wrapper));
    if (wrapper == NULL) {
        return -1;
    }
    wrapper->directory =
        tsk_fs_dir_open_meta(filesystem->filesystem, (TSK_INUM_T) inode);
    if (wrapper->directory == NULL) {
        free(wrapper);
        return -1;
    }
    *output = wrapper;
    return 0;
}

void ofi_tsk_dir_close(ofi_tsk_dir *directory) {
    if (directory != NULL) {
        if (directory->directory != NULL) {
            tsk_fs_dir_close(directory->directory);
        }
        free(directory);
    }
}

size_t ofi_tsk_dir_count(const ofi_tsk_dir *directory) {
    return directory != NULL && directory->directory != NULL
        ? tsk_fs_dir_getsize(directory->directory) : 0;
}

static void copy_file_info(const TSK_FS_FILE *file, ofi_file_info *output) {
    const TSK_FS_NAME *name;
    const TSK_FS_META *meta;
    memset(output, 0, sizeof(*output));
    if (file == NULL) {
        return;
    }
    name = file->name;
    if (name != NULL) {
        if (name->name != NULL) {
            size_t length = strlen(name->name);
            size_t copy_length = length < sizeof(output->name) - 1
                ? length : sizeof(output->name) - 1;
            memcpy(output->name, name->name, copy_length);
            output->name[copy_length] = '\0';
            output->name_truncated = length >= sizeof(output->name);
        }
        output->metadata_address = name->meta_addr;
        output->parent_address = name->par_addr;
        output->name_flags = (uint32_t) name->flags;
        output->name_type = (uint32_t) name->type;
    }
    meta = file->meta;
    if (meta != NULL) {
        output->metadata_address = meta->addr;
        output->size = (uint64_t) meta->size;
        output->metadata_flags = (uint32_t) meta->flags;
        output->metadata_type = (uint32_t) meta->type;
        output->mode = (uint32_t) meta->mode;
        output->link_count = meta->nlink;
        output->uid = (uint32_t) meta->uid;
        output->gid = (uint32_t) meta->gid;
        output->atime = (int64_t) meta->atime;
        output->mtime = (int64_t) meta->mtime;
        output->ctime = (int64_t) meta->ctime;
        output->crtime = (int64_t) meta->crtime;
        output->atime_nano = meta->atime_nano;
        output->mtime_nano = meta->mtime_nano;
        output->ctime_nano = meta->ctime_nano;
        output->crtime_nano = meta->crtime_nano;
    }
}

int ofi_tsk_dir_entry(const ofi_tsk_dir *directory, size_t index,
                      ofi_file_info *entry) {
    TSK_FS_FILE *file;
    if (directory == NULL || directory->directory == NULL || entry == NULL) {
        return -1;
    }
    tsk_error_reset();
    file = tsk_fs_dir_get(directory->directory, index);
    if (file == NULL) {
        return -1;
    }
    copy_file_info(file, entry);
    tsk_fs_file_close(file);
    return 0;
}

int ofi_tsk_file_open(const ofi_tsk_fs *filesystem, uint64_t inode,
                      ofi_tsk_file **output) {
    struct ofi_tsk_file *wrapper;
    if (filesystem == NULL || filesystem->filesystem == NULL || output == NULL) {
        return -1;
    }
    *output = NULL;
    tsk_error_reset();
    wrapper = (struct ofi_tsk_file *) calloc(1, sizeof(*wrapper));
    if (wrapper == NULL) {
        return -1;
    }
    wrapper->file = tsk_fs_file_open_meta(filesystem->filesystem, NULL,
                                          (TSK_INUM_T) inode);
    if (wrapper->file == NULL) {
        free(wrapper);
        return -1;
    }
    *output = wrapper;
    return 0;
}

void ofi_tsk_file_close(ofi_tsk_file *file) {
    if (file != NULL) {
        if (file->file != NULL) {
            tsk_fs_file_close(file->file);
        }
        free(file);
    }
}

ssize_t ofi_tsk_file_read(const ofi_tsk_file *file, uint64_t offset,
                          void *buffer, size_t length) {
    if (file == NULL || file->file == NULL || buffer == NULL) {
        return -1;
    }
    tsk_error_reset();
    return tsk_fs_file_read(file->file, (TSK_OFF_T) offset, (char *) buffer,
                            length, TSK_FS_FILE_READ_FLAG_NONE);
}

int ofi_tsk_file_info(const ofi_tsk_file *file, ofi_file_info *entry) {
    if (file == NULL || file->file == NULL || entry == NULL) {
        return -1;
    }
    copy_file_info(file->file, entry);
    return 0;
}
