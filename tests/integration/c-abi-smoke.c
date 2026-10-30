#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "ofi.h"

int main(void) {
    char db_path[256];
    snprintf(db_path, sizeof(db_path), "/tmp/openforensic-c-abi-%ld.db", (long)getpid());
    unlink(db_path);

    char request[1024];
    snprintf(
        request,
        sizeof(request),
        "{\"db_path\":\"%s\",\"id\":\"00000000-0000-0000-0000-000000000001\","
        "\"title\":\"C ABI\",\"examiner\":\"Examiner\"}",
        db_path);

    unsigned char *response = NULL;
    size_t response_len = 0;
    ofi_error *error = NULL;
    ofi_status status = ofi_case_create(
        (const unsigned char *)request,
        strlen(request),
        &response,
        &response_len,
        &error);
    if (status != OFI_STATUS_OK || response == NULL || response_len == 0 || error != NULL) {
        fprintf(stderr, "case creation failed: %d\n", status);
        if (error != NULL) {
            ofi_error_free(error);
        }
        return 1;
    }

    if (memmem(response, response_len, "\"ok\":true", 8) == NULL) {
        fprintf(stderr, "missing success envelope\n");
        ofi_bytes_free(response, response_len);
        return 1;
    }
    ofi_bytes_free(response, response_len);
    unlink(db_path);
    return 0;
}
