#ifndef OPENFORENSIC_IMAGER_OFI_H
#define OPENFORENSIC_IMAGER_OFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum ofi_status {
    OFI_STATUS_OK = 0,
    OFI_STATUS_INVALID_ARGUMENT = 1,
    OFI_STATUS_INVALID_JSON = 2,
    OFI_STATUS_OPERATION_FAILED = 3,
    OFI_STATUS_PANIC = 4
} ofi_status;

typedef struct ofi_error ofi_error;

uint32_t ofi_abi_version(void);

ofi_status ofi_case_create(
    const uint8_t *request,
    size_t request_len,
    uint8_t **response,
    size_t *response_len,
    ofi_error **error);

ofi_status ofi_evidence_add(
    const uint8_t *request,
    size_t request_len,
    uint8_t **response,
    size_t *response_len,
    ofi_error **error);

void ofi_bytes_free(uint8_t *bytes, size_t len);
void ofi_error_free(ofi_error *error);
const char *ofi_error_code(const ofi_error *error);
const char *ofi_error_message(const ofi_error *error);

#ifdef __cplusplus
}
#endif

#endif

