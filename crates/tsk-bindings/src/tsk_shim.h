#ifndef OPENFORENSIC_IMAGER_TSK_SHIM_H
#define OPENFORENSIC_IMAGER_TSK_SHIM_H

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

typedef struct ofi_tsk_image ofi_tsk_image;
typedef struct ofi_tsk_vs ofi_tsk_vs;
typedef struct ofi_tsk_fs ofi_tsk_fs;
typedef struct ofi_tsk_dir ofi_tsk_dir;
typedef struct ofi_tsk_file ofi_tsk_file;

typedef struct ofi_partition_info {
    uint64_t address;
    uint64_t start_sector;
    uint64_t sector_count;
    uint64_t offset_bytes;
    uint64_t length_bytes;
    uint32_t flags;
    int8_t table_num;
    int8_t slot_num;
    char description[256];
} ofi_partition_info;

typedef struct ofi_file_info {
    char name[4096];
    uint32_t name_truncated;
    uint64_t metadata_address;
    uint64_t parent_address;
    uint64_t size;
    uint32_t name_flags;
    uint32_t metadata_flags;
    uint32_t name_type;
    uint32_t metadata_type;
    uint32_t mode;
    int32_t link_count;
    uint32_t uid;
    uint32_t gid;
    int64_t atime;
    int64_t mtime;
    int64_t ctime;
    int64_t crtime;
    uint32_t atime_nano;
    uint32_t mtime_nano;
    uint32_t ctime_nano;
    uint32_t crtime_nano;
} ofi_file_info;

void ofi_tsk_last_error(char *buffer, size_t size);

int ofi_tsk_image_open(const char *path, ofi_tsk_image **image);
void ofi_tsk_image_close(ofi_tsk_image *image);
uint64_t ofi_tsk_image_size(const ofi_tsk_image *image);
uint32_t ofi_tsk_image_sector_size(const ofi_tsk_image *image);

int ofi_tsk_vs_open(const ofi_tsk_image *image, uint8_t type, ofi_tsk_vs **volume_system);
void ofi_tsk_vs_close(ofi_tsk_vs *volume_system);
uint32_t ofi_tsk_vs_type(const ofi_tsk_vs *volume_system);
uint32_t ofi_tsk_vs_block_size(const ofi_tsk_vs *volume_system);
uint32_t ofi_tsk_vs_partition_count(const ofi_tsk_vs *volume_system);
int ofi_tsk_vs_partition(const ofi_tsk_vs *volume_system, uint32_t index,
                         ofi_partition_info *partition);

int ofi_tsk_fs_open(const ofi_tsk_image *image, uint64_t offset_bytes,
                    ofi_tsk_fs **filesystem);
void ofi_tsk_fs_close(ofi_tsk_fs *filesystem);
uint32_t ofi_tsk_fs_type(const ofi_tsk_fs *filesystem);
uint32_t ofi_tsk_fs_block_size(const ofi_tsk_fs *filesystem);
uint64_t ofi_tsk_fs_inode_count(const ofi_tsk_fs *filesystem);
uint64_t ofi_tsk_fs_root_inode(const ofi_tsk_fs *filesystem);

int ofi_tsk_dir_open(const ofi_tsk_fs *filesystem, uint64_t inode,
                     ofi_tsk_dir **directory);
void ofi_tsk_dir_close(ofi_tsk_dir *directory);
size_t ofi_tsk_dir_count(const ofi_tsk_dir *directory);
int ofi_tsk_dir_entry(const ofi_tsk_dir *directory, size_t index,
                      ofi_file_info *entry);

int ofi_tsk_file_open(const ofi_tsk_fs *filesystem, uint64_t inode,
                      ofi_tsk_file **file);
void ofi_tsk_file_close(ofi_tsk_file *file);
ssize_t ofi_tsk_file_read(const ofi_tsk_file *file, uint64_t offset,
                          void *buffer, size_t length);
int ofi_tsk_file_info(const ofi_tsk_file *file, ofi_file_info *entry);

#endif
