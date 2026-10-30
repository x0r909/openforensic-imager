# Compatibility Matrix

## Status definitions

| Status | Meaning |
|---|---|
| Documented | Described by Exterro documentation |
| FTK runtime | Directly executed and observed in an authorized FTK environment |
| Independent candidate | OpenForensic can implement the workflow independently |
| Implemented | OpenForensic code exists |
| Tested | OpenForensic test evidence passes |
| Partial | Supported with a documented limitation |
| Unsupported | Not claimed |

No FTK runtime check was performed. No OpenForensic feature is implemented in
Phase 0.

## Workflow matrix

| Capability | FTK documented | FTK runtime | OpenForensic target | Current OFI state | Required evidence |
|---|---|---|---|---|---|
| Add physical device | Yes | Unknown | Yes | Not implemented | Read-only source tests |
| Add logical volume | Yes | Unknown | Yes | Not implemented | Read-only source tests |
| Add RAW image | Yes | Unknown | Yes | Not implemented | RAW reader tests |
| Add E01 image | Yes | Unknown | Yes | Not implemented | libewf round trip and corruption tests |
| Add segmented E01 | Yes | Unknown | Yes | Not implemented | Segment inventory tests |
| Browse MBR | Implied by disk analysis | Unknown | Yes | Not implemented | MBR differential tests |
| Browse GPT | Implied by disk analysis | Unknown | Yes | Not implemented | GPT differential tests |
| NTFS browsing | Yes | Unknown | Yes | Not implemented | TSK plus ntfs-3g differential |
| FAT32 browsing | Yes | Unknown | Yes | Not implemented | TSK plus mtools/dosfstools |
| exFAT browsing | Yes | Unknown | Yes | Not implemented | TSK plus exfatprogs |
| ext4 browsing | Yes | Unknown | Yes | Not implemented | TSK plus debugfs |
| Deleted names | Yes | Unknown | Yes where parser exposes | Not implemented | Deletion fixtures per FS |
| Deleted content recovery | Yes, limited claim | Unknown | Conditional | Not implemented | Reused-block and retained-block fixtures |
| Hex view | Yes | Unknown | Yes | Not implemented | Paged hex model tests |
| Text view | Yes | Unknown | Yes | Not implemented | Encoding and budget tests |
| Image preview | Via natural mode | Unknown | Yes, bounded | Not implemented | Image bomb tests |
| PDF preview | Not established | Unknown | Optional, safe | Not implemented | Isolated parser tests |
| Physical acquisition | Yes | Unknown | Yes, with write blocker | Not implemented | Injected read-error tests |
| Logical acquisition | Yes | Unknown | Yes | Not implemented | Source policy tests |
| RAW output | Yes | Unknown | Yes | Not implemented | Byte equality and hash tests |
| E01 output | Yes | Unknown | Yes, EWF1 | Not implemented | libewf and differential tests |
| S01 output | Yes | Unknown | No initial | Unsupported | Deferred |
| AFF output | Yes | Unknown | No initial | Unsupported | Deferred |
| AD1 output | Yes | Unknown | No initial | Unsupported | Deferred |
| Compression | Yes | Unknown | E01 | Not implemented | Compression matrix tests |
| Segment size | Yes | Unknown | RAW split and E01 | Not implemented | Boundary tests |
| Encrypted output | Yes for several formats | Unknown | No initial | Unsupported | Deferred credential design |
| MD5 image hash | Yes | Unknown | Yes | Not implemented | Known vectors and E01 |
| SHA-1 image hash | Yes | Unknown | Yes | Not implemented | Known vectors and E01 |
| SHA-256 image hash | Not established in guide workflow | Unknown | Yes | Not implemented | Known vectors and independent tools |
| SHA-512 image hash | Not established in guide workflow | Unknown | Yes | Not implemented | Known vectors and independent tools |
| Verify stored E01 hash | Yes | Unknown | Yes | Not implemented | Match/mismatch/missing tests |
| Bad-sector count | Yes | Unknown | Yes | Not implemented | Fault injection |
| Exact unreadable ranges | Not established | Unknown | Yes | Not implemented | Fault injection |
| Export individual file | Yes | Unknown | Yes | Not implemented | Byte/hash tests |
| Export directory | Yes | Unknown | Yes | Not implemented | Traversal and conflict tests |
| Preserve evidence | Yes as claim | Unknown | Yes, tested | Not implemented | Source hash unchanged tests |
| Acquisition report | Image summary | Unknown | Yes | Not implemented | Report schema tests |
| Verification report | Results screen | Unknown | Yes | Not implemented | Report schema tests |
| Append-only audit | Not established for free Imager | Unknown | Yes | Not implemented | Chain and tamper tests |
| Read-only mount | Yes | Unknown | No initial | Unsupported | Deferred mount design |
| External viewer | Documented | Unknown | No | Unsupported | Security decision |

## Format matrix

| Item | FTK documented | FTK runtime | OpenForensic 0.1 | Current state |
|---|---|---|---|---|
| RAW/DD read | Yes | Unknown | Required | Not implemented |
| RAW/DD write | Yes | Unknown | Required | Not implemented |
| EWF1 E01 read | Yes | Unknown | Required | Not implemented |
| EWF1 E01 write | Yes | Unknown | Required | Not implemented |
| EWF1 S01 read | Yes | Unknown | Optional | Not implemented |
| EWF1 S01 write | Yes | Unknown | Unsupported | Not implemented |
| EWF2 Ex01 read | Not established in 8.2 guide | Unknown | Experimental only | Not implemented |
| EWF2 Ex01 write | Not established | Unknown | Unsupported | Not implemented |
| MBR | Implied | Unknown | Required | Not implemented |
| GPT | Implied | Unknown | Required | Not implemented |
| NTFS | Yes | Unknown | Required | Not implemented |
| FAT32 | Yes | Unknown | Required | Not implemented |
| exFAT | Yes | Unknown | Required | Not implemented |
| ext4 | Yes | Unknown | Required | Not implemented |
| AFF, AD1, L01 | Yes | Unknown | Unsupported initially | Not implemented |

## Compatibility claims allowed

Allowed after tests:

- "Reads EWF1 E01 images with libewf version X."
- "Creates RAW images whose SHA-256 equals the tested source-read stream."
- "Lists NTFS files on tested fixture images."
- "Extracts tested files without modification and records SHA-256."

Not allowed:

- "FTK Imager compatible."
- "Drop-in replacement."
- "Supports all E01 images."
- "Recovers deleted files."
- "Forensically sound on all hardware."
- "Verifies SHA-256 in E01."

Each compatibility statement must identify the exact format, version, test
fixture class, parser version, and limitation.

