# Unknowns and Follow-up Work

## Priority definitions

- P0: blocks a production claim or could compromise evidence integrity.
- P1: needed for a planned release feature.
- P2: useful for broader compatibility.

## Unknown register

| ID | Priority | Unknown | Why unresolved | Resolution |
|---|---|---|---|---|
| U-001 | P0 | Actual FTK 8.3 application module architecture | Installer payload is opaque; no authorized Windows runtime was used | Static analysis of an authorized extracted installation or runtime module capture |
| U-002 | P0 | FTK behavior on corrupt or truncated E01 images | No runtime fault-injection evidence | Execute in authorized Windows test environment and record every state |
| U-003 | P0 | FTK behavior on unreadable source sectors | No hardware fault injection or runtime capture | Use disposable disk and controlled bad sectors in a lab |
| U-004 | P1 | Exact current FTK 8.3 UI and interaction model | Only 8.2 user guide and product pages were available | Capture 8.3.1.6 on Windows |
| U-005 | P1 | FTK error taxonomy, precedence, and retry behavior | Not documented at required depth | Inject failures and capture dialogs/logs |
| U-006 | P1 | FTK's full EWF variant support and write defaults | Documentation lists formats but not detailed writer behavior | Create comparative fixture matrix on authorized runtime |
| U-007 | P1 | FTK's exact deleted-file semantics across filesystems | Documented capability is broad and not per-FS | Acquire controlled deleted fixtures and compare output |
| U-008 | P1 | Whether FTK has an append-only chain-of-custody log beyond acquisition summary | Free Imager documentation is insufficient | Inspect 8.3 runtime output and optional suite integration |
| U-009 | P1 | Exact timestamp timezone assumptions in FTK views and reports | 8.2 guide does not document per-format conversion | Controlled timezone and DST fixtures |
| U-010 | P1 | libewf EWF2 write maturity for the exact pinned version | Upstream README and ChangeLog conflict | Build and run EWF2 round-trip, corruption, and tool differential tests |
| U-011 | P1 | libewf thread-safety and concurrent-handle behavior | Upstream lists multi-threading as work in progress | Stress tests with multiple read handles and segments |
| U-012 | P1 | TSK semantic correctness for all ext4 feature combinations | Basic ext4 fixture passed; feature matrix incomplete | Generate feature matrix and differential fixtures |
| U-013 | P1 | exFAT UTC offset recovery through available libraries | Current TSK path uses local time and does not apply offset in inspected conversion | Add raw-field test, upstream fix, or independent metadata adapter |
| U-014 | P1 | Exact NTFS timestamp range and nanosecond mapping in pinned TSK | Source shows a 32-bit range clamp; runtime tests are missing | Boundary fixtures before 1970, around 2106, and subsecond values |
| U-015 | P1 | Ext4 post-2038 timestamp handling in pinned TSK | Source does not apply epoch bits in inspected path | Runtime boundary tests and upstream resolution |
| U-016 | P0 | Hardware write-blocker attestation interface | No universal Linux API was identified | Manual attestation field plus documented device evidence; optional vendor integration later |
| U-017 | P1 | Flatpak physical acquisition feasibility | Device and filesystem permissions are not yet tested | Build Flatpak and run acquisition in a disposable VM |
| U-018 | P1 | AppImage compatibility across supported distros | Tooling absent; libraries not bundled yet | Build and test on clean VMs |
| U-019 | P2 | Optional libfsntfs benefits over TSK | Library not installed; no comparative tests | Add as differential and specialized attribute engine |
| U-020 | P2 | libvshadow, libvhdi, libvmdk integration scope | Formats are not in the initial release | Define separate feature specs and tests |
| U-021 | P0 | Project license and distribution obligations | Application license not selected; native dependency licenses differ | Legal review and distribution decision |
| U-022 | P1 | Reproducible native build of pinned TSK and libewf on all distros | Only package builds were inspected | Build matrix with hashes and compiler/toolchain capture |
| U-023 | P1 | Exact safe PDF preview implementation | Qt PDF module absent | Install optional module, design sandbox, fuzz |
| U-024 | P1 | Large-directory performance and memory behavior | Only small fixtures tested | Multi-million-entry fixtures and benchmarks |
| U-025 | P2 | ARM64 behavior of native dependencies and arithmetic | No ARM64 host tested | Cross-build and ARM64 test runner |

## Evidence needed to close unknowns

- Authorized Windows FTK runtime with disposable images.
- Pinned native dependency builds with symbol and version manifests.
- Fault-injection storage for unreadable-sector behavior.
- Timezone-controlled filesystem fixtures.
- Corrupt EWF corpus and differential tool results.
- Clean VM matrix for packaging.

## Unknown handling policy

- An unknown cannot be represented as a supported feature.
- A UI label must distinguish unavailable, partial, unsupported, and failed.
- Reports must carry the same state as the UI.
- Closing an unknown requires a recorded reproduction and test, not an inference
  from marketing copy.

