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

No FTK runtime check was performed. This matrix records the tested core-library
state after Phase 4. The desktop UI and later workflow modules are not complete.

## Workflow matrix

| Capability | FTK documented | FTK runtime | OpenForensic target | Current OFI state | Required evidence |
|---|---|---|---|---|---|
| Add physical device | Yes | Unknown | Yes | Not implemented | Read-only source tests |
| Add logical volume | Yes | Unknown | Yes | Not implemented | Read-only source tests |
| Add RAW image | Yes | Unknown | Yes | Tested core reader | RAW reader tests |
| Add E01 image | Yes | Unknown | Yes | Tested core reader (EWF1) | libewf round trip and corruption tests |
| Add segmented E01 | Yes | Unknown | Yes | Tested core reader | Segment inventory tests |
| Browse MBR | Implied by disk analysis | Unknown | Yes | Tested core analyzer | MBR differential tests |
| Browse GPT | Implied by disk analysis | Unknown | Yes | Tested core analyzer | GPT differential tests |
| NTFS browsing | Yes | Unknown | Yes | Tested core explorer | TSK plus ntfs-3g differential |
| FAT32 browsing | Yes | Unknown | Yes | Tested core explorer | TSK plus mtools/dosfstools |
| exFAT browsing | Yes | Unknown | Yes | Partial: root listing tested | TSK plus exfatprogs |
| ext4 browsing | Yes | Unknown | Yes | Tested core explorer | TSK plus debugfs |
| Deleted names | Yes | Unknown | Yes where parser exposes | Partial: ext4 status fixture | Deletion fixtures per FS |
| Deleted content recovery | Yes, limited claim | Unknown | Conditional | Not implemented | Reused-block and retained-block fixtures |
| Hex view | Yes | Unknown | Yes | Tested paged core model | Paged hex model tests |
| Text view | Yes | Unknown | Yes | Tested bounded core model | Encoding and budget tests |
| Image preview | Via natural mode | Unknown | Yes, bounded | Partial: preflight only | Image bomb tests |
| PDF preview | Not established | Unknown | Optional, safe | Partial: size preflight only | Isolated parser tests |
| Physical acquisition | Yes | Unknown | Yes, with write blocker | Not implemented | Injected read-error tests |
| Logical acquisition | Yes | Unknown | Yes | Not implemented | Source policy tests |
| RAW output | Yes | Unknown | Yes | Tested for regular-file sources | Byte equality and hash tests |
| E01 output | Yes | Unknown | Yes, EWF1 | Tested for regular-file sources | libewf and differential tests |
| S01 output | Yes | Unknown | No initial | Unsupported | Deferred |
| AFF output | Yes | Unknown | No initial | Unsupported | Deferred |
| AD1 output | Yes | Unknown | No initial | Unsupported | Deferred |
| Compression | Yes | Unknown | E01 | Not implemented | Compression matrix tests |
| Segment size | Yes | Unknown | RAW split and E01 | Tested E01 segmentation | Boundary tests |
| Encrypted output | Yes for several formats | Unknown | No initial | Unsupported | Deferred credential design |
| MD5 image hash | Yes | Unknown | Yes | Tested | Known vectors and E01 |
| SHA-1 image hash | Yes | Unknown | Yes | Tested | Known vectors and E01 |
| SHA-256 image hash | Not established in guide workflow | Unknown | Yes | Tested | Known vectors and independent tools |
| SHA-512 image hash | Not established in guide workflow | Unknown | Yes | Tested | Known vectors and independent tools |
| Verify stored E01 hash | Yes | Unknown | Yes | Tested match/mismatch/unavailable | Match/mismatch/missing tests |
| Bad-sector count | Yes | Unknown | Yes | Tested with injected sources | Fault injection |
| Exact unreadable ranges | Not established | Unknown | Yes | Tested with injected sources | Fault injection |
| Export individual file | Yes | Unknown | Yes | Tested core extractor | Byte/hash tests |
| Export directory | Yes | Unknown | Yes | Tested core extractor | Traversal and conflict tests |
| Preserve evidence | Yes as claim | Unknown | Yes, tested | Tested source hash unchanged | Source hash unchanged tests |
| Acquisition report | Image summary | Unknown | Yes | Not implemented | Report schema tests |
| Verification report | Results screen | Unknown | Yes | Not implemented | Report schema tests |
| File extraction report | Yes | Unknown | Yes | Tested JSON/CSV/HTML | Report schema and escaping tests |
| Append-only audit | Not established for free Imager | Unknown | Yes | Tested core log | Chain and tamper tests |
| Read-only mount | Yes | Unknown | No initial | Unsupported | Deferred mount design |
| External viewer | Documented | Unknown | No | Unsupported | Security decision |

## Format matrix

| Item | FTK documented | FTK runtime | OpenForensic 0.1 | Current state |
|---|---|---|---|---|
| RAW/DD read | Yes | Unknown | Required | Tested |
| RAW/DD write | Yes | Unknown | Required | Tested for regular-file sources |
| EWF1 E01 read | Yes | Unknown | Required | Tested with host libewf |
| EWF1 E01 write | Yes | Unknown | Required | Tested with host libewf |
| EWF1 S01 read | Yes | Unknown | Optional | Not implemented |
| EWF1 S01 write | Yes | Unknown | Unsupported | Not implemented |
| EWF2 Ex01 read | Not established in 8.2 guide | Unknown | Experimental only | Not implemented |
| EWF2 Ex01 write | Not established | Unknown | Unsupported | Not implemented |
| MBR | Implied | Unknown | Required | Tested |
| GPT | Implied | Unknown | Required | Tested |
| NTFS | Yes | Unknown | Required | Tested basic browse/read |
| FAT32 | Yes | Unknown | Required | Tested basic browse/read |
| exFAT | Yes | Unknown | Required | Partial: root listing |
| ext4 | Yes | Unknown | Required | Tested basic browse/read |
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

