# Worker privilege boundary

Status: **implemented (2026-11-06).** The dedicated `ofi-worker` system user
recommendation below was implemented, and residual risk 1 in
[phase8-review.md](phase8-review.md) is recorded as resolved. The worker drops
to that low-privilege account after Landlock grants and before parsing evidence;
see `phase8-review.md` for the implementation and test evidence. This document
now records the decision and rationale leading to that implementation.

## The question

Residual risk 1: the parser worker runs as the examiner's Unix user, so a
kernel or sandbox escape, or a defect in a native parser that yields arbitrary
code execution, compromises that user's full authority rather than a lesser
one. Everything the worker already has — Landlock, the default-deny seccomp
allow-list, rlimits, the environment allowlist, the response cap, the watchdog
— is defense against the *worker* misbehaving. None of it survives an escape
from the worker's own process, and none of it claims to.

The question this document answers: should the worker become a privilege
boundary, and if so, which one.

## What the worker can reach today

The examiner's authority is what a compromised worker inherits. Measured on the
reference host with `id`, `/proc/self/status`, and `ls`:

- uid 1000, and supplementary groups including `adm` (4), `cdrom` (24),
  `sudo` (27), `dip` (30), `plugdev` (46), `users` (100), `lpadmin` (111),
  `lxd` (114), and `docker` (973).
- `CapEff == 0`, so no effective capabilities — but `CapBnd` is
  `000001ffffffffff`, the full bounding set. Anything that lets the worker
  execute a file with a file capability on it, or reach a setuid binary through
  an escape, starts from an unprejudiced bounding set.
- `/var/log/auth.log` (owner `syslog`, group `adm`, mode 0640) opens, because
  the examiner is in `adm`. `/etc/shadow` and `/root` do not.
- `/var/run/docker.sock` (mode `0660`, group `docker`) is reachable, because
  the examiner is in `docker`. On this host that is root-equivalent by itself.

The evidence files being parsed are, by the nature of the application, hostile
input. The examiner's session is the thing worth protecting: the case database
and audit log, the examiner's documents, and the credentials and sockets
reachable from a desktop session.

## Reference host

Measurements below are from this host, the one README.md records as the
baseline environment (Ubuntu 26.04.1 LTS, x86_64). The kernel has moved since
that baseline was recorded on 2026-10-30, from `7.0.0-38-generic` to
`7.0.0-42-generic`; no measurement below depends on the difference.

```
Ubuntu 26.04.1 LTS, Linux 7.0.0-42-generic, x86_64
kernel.apparmor_restrict_unprivileged_userns = 1
AppArmor enabled; /etc/apparmor.d/unprivileged_userns and
/etc/apparmor.d/bwrap-userns-restrict both present
/usr/bin/bwrap = bubblewrap 0.11.1 (distro package)
Landlock ABI 8
```

Cross-distro variation is the reason several findings below are stated as
measurements with their method rather than as mechanics, and the one mechanic
left unexplained is called out as such.

## Options

Four are real. A fifth ("run the worker as root and drop privileges") is not:
running as root is already discouraged by [SECURITY_MODEL.md](../../SECURITY_MODEL.md)
§ 6, and dropping *from* root means the supervisor holds root, which is worse
than what it replaces.

### A. User namespace with the examiner's uid mapped to root

The supervisor forks, the child calls `unshare(CLONE_NEWUSER)`, the supervisor
writes the child's `uid_map`/`gid_map`, and the child continues with euid 0
inside a namespace where uid 0 *is* the examiner.

**What it blocks that the others do not.** Nothing, alone. Measured, and this
is the finding that decides against it as a primary boundary: mapped root is
the examiner's uid in the parent namespace, so every file owned by the examiner
— the case database, the audit log, the examiner's documents — is still
readable and writable. It does not reduce authority over the examiner's own
data at all. What it does change: `/etc/shadow` and `/root` are denied where
the examiner is also denied them (no change), and supplementary-group authority
survives through a hazard described under B.

### B. Mount namespace (the `bwrap` shape)

Fork, `CLONE_NEWUSER | CLONE_NEWNS`, map in the supervisor, mount a private
root holding only the paths the job needs, `pivot_root`, and let the worker see
no host path that was not bound in.

**What it blocks that the others do not.** This is the only option that
protects by *construction* rather than by policy: after `pivot_root` the host
filesystem is not reachable, so a Landlock or seccomp escape lands in a
namespace with nothing in it. It also isolates PID and network namespaces,
which removes visibility of and signalling to the examiner's processes.

**Measured, on this host:**

```
unshare --user --map-root-user true
  -> unshare: write failed /proc/self/uid_map: Operation not permitted
bwrap --unshare-all --die-with-parent --ro-bind / / --proc /proc --uid 0 --gid 0 true
  -> ok; child shows Uid 0, CapEff/CapBnd/CapPrm = 0000000000000000, NoNewPrivs 1
cp /usr/bin/bwrap /tmp/my-bwrap && /tmp/my-bwrap --unshare-user --ro-bind / / sh -c 'id -u'
  -> bwrap: setting up uid map: Permission denied
```

So the stock `bwrap` works and an identical copy of the same binary does not.
The stock binary is covered by the distro's `bwrap-userns-restrict` AppArmor
profile; the copy is not. The mechanism — whether the profile is what makes
bwrap's own mapping path legal, and what exactly the policy refuses without it
— is **not fully explained here**, and this document does not rely on a
mechanism it cannot demonstrate.

What it does rely on is that a supervisor *writing its own child's* map is a
different path from a process mapping itself. Measured with a forked child that
unshares and then stops itself, with the parent writing `/proc/<pid>/setgroups`,
`uid_map`, and `gid_map`:

```
PARENT: setgroups=ok uid_map(rc=8)=ok gid_map(rc=8)=ok
CHILD: euid=0 gid=0
```

That succeeds from an unprofiled binary in `/tmp` on this host. It is the
path a supervisor would take: fork, child unshares and stops, parent maps,
child continues and execs. **Any plan built on this option rests on that
measurement holding on every supported distribution**, and the failure mode
when it does not is a worker that cannot be sandboxed at all — which must fail
closed, not silently run in the current model.

**Two hazards measured for A and B alike, both of which narrow the benefit:**

Unmapped supplementary groups collapse to 65534 rather than disappearing:

```
examiner: egid=1000 ngroups=10 [4 24 27 30 46 100 111 114 973 1000]
inside a mapped userns: egid=0 ngroups=10 [65534 65534 65534 65534 ... 0]
```

and the file that the examiner reached through `adm` is still open in the
child, because its unmapped group owner (4) also reads as 65534:

```
PARENT /var/log/auth.log  OPEN
CHILD  /var/log/auth.log  OPEN
PARENT /etc/shadow        DENIED
CHILD  /etc/shadow        DENIED
```

So a uid mapping does not strip group authority; a `subuid`-style mapping or an
explicit group map is needed to do that, and neither is free.

Inside the mapped namespace the child holds a full capability set *within that
namespace* (`CapEff: 000001ffffffffff` in the child while the examiner has
`0`). That is correct kernel behaviour and is not an escape — the capabilities
do not extend to the parent namespace, and `CapBnd` at the boundary is `0` in
the bwrap case. It does mean the worker's seccomp filter is the only thing
constraining mount and namespace syscalls inside its own namespace, which is
the current situation anyway.

### C. Dedicated system user, `setuid` at spawn

A `ofi-worker` account owns the evidence-parsing path; the supervisor switches
uid before `exec`.

**What it blocks that the others do not.** The one property that actually
matters for a forensic tool: a compromised parser cannot reach the examiner's
case database, audit log, or documents, cannot write to them, and cannot reach
the examiner's desktop credentials or group-owned sockets. A/B leave all of
that intact or nearly so. It is a real privilege boundary in the sense residual
risk 1 means, and it is stable across distributions because it depends on the
kernel's uids, not on an LSM policy.

**What it costs.** Everything else in this section is cheap next to it:

- Packaging. The DEB can create the user in a maintainer script; the AppImage
  cannot (it is a portable single-file artifact with no install step), and the
  Flatpak already has its own sandbox and cannot switch to a host user.
- The supervisor must hold the authority to switch — a setuid helper, a systemd
  unit, or a capability on the launcher. Each is a new privileged artifact, and
  a setuid helper in the desktop's path is a worse attack surface than the
  worker it protects.
- Output. The worker writes acquisition and extraction output directly into the
  examiner's destination directory today. A separate uid means the supervisor
  must arrange group-writable staging and own the final placement, which is a
  change to the acquisition output path and to its tests.
- Every existing "not a privilege boundary" claim becomes wrong and has to move
  with it, for the artifact C covers: `SECURITY_MODEL.md` § 7,
  `docs/specifications/interfaces.md`, `docs/packaging/INSTALL.md`,
  [phase6-review.md](phase6-review.md), and [phase8-review.md](phase8-review.md).

### D. Keep the current model

No new mechanism. Landlock and seccomp continue to constrain the worker;
authority after an escape remains the examiner's.

**What it costs.** Residual risk 1 stands unchanged and must keep being stated
plainly wherever the boundary is described. The mitigation available today is
operational, not technical: run the imager in a session with no `docker` or
`sudo` group membership and no access to case data while parsing hostile
evidence.

## Recommendation

**Adopt C — a dedicated system user — as the follow-up, scoped to the DEB; keep
D as the shipped behaviour until it lands; do not adopt A or B as the boundary.**

The reasoning, in order:

1. The threat that matters for this application is a parser compromise reaching
   the case record. Only C removes that. A was measured not to reduce authority
   over the examiner's own files at all, and B's filesystem isolation protects
   the *rest* of the system while leaving the case database readable by mapped
   root.
2. B is a genuine second layer and the strongest one against a Landlock escape,
   but it rests on unprivileged user-namespace creation succeeding, which was
   measured to fail here for a binary without a policy that grants it. Building
   the primary boundary on a mechanism the distribution already refuses by
   default is the wrong order of dependencies.
3. C's costs are all known engineering — packaging, a privileged spawn, and the
   output path — and none of them is a research question. That is what makes it
   a plan rather than a project.
4. A and B are not wasted work if they come later: B composes with C, and the
   supervisor-side mapping measurement above is the piece a B plan would need.

**What C does not fix, stated plainly.** The Flatpak and the AppImage keep the
current model. Their assurance is lower than the DEB's until each gets its own
answer — for the Flatpak, that its own sandbox already denies most of what C
denies; for the AppImage, that a portable artifact has no way to own a system
account. Any release text must say which artifact the boundary applies to.
C also does not protect the worker from a kernel-level escape gaining root —
no userspace option does.

**Cost of being wrong.**

- If C is implemented and the threat is smaller than assessed: the cost is the
  packaging, privilege, and output-path work, plus a setuid or systemd artifact
  that is itself something to attack. Reversible by shipping D again.
- If C is *not* implemented and residual risk 1 is realised against an examiner
  parsing hostile evidence: the loss is the case record's integrity and the
  examiner's session, in a tool whose entire output is a claim about integrity.
  That asymmetry is why C is recommended despite the machinery.
- If the reasoning on A is wrong — that is, if mapped root does reduce
  authority over the examiner's files somewhere this measurement missed — the
  consequence is a cheaper path to the same boundary having been passed over,
  not a wrong boundary.

## What would change this recommendation

- A measured demonstration that unprivileged user-namespace setup succeeds on
  every supported distribution, which would move B into the recommended set
  alongside C.
- A packaging decision that the AppImage and Flatpak must carry the same
  assurance as the DEB, which makes C's scope the whole problem rather than the
  DEB alone.
- Evidence that the examiner's group membership is already minimal in the
  supported deployment — which would weaken the case-record argument for C and
  make D defensible as the answer rather than a placeholder.

## The decision the examiner had to make

This was the handoff Step 3 of the task called for. The three questions were
answered as follows when C was implemented on 2026-11-06:

1. **Scope.** C is implemented for the DEB path, where the `ofi-worker` system
   user exists. The AppImage and Flatpak are documented as reduced assurance
   because the dedicated user is not present in those formats, and the worker
   falls back to UID 65534 (nobody).
2. **Privileged spawn mechanism.** Implemented as `setgroups(0, NULL)`,
   `setgid`, then `setuid` inside the worker process, applied by
   `worker_sandbox::apply_privilege_drop` after Landlock grants are
   established and before seccomp and parsing (`parser_worker.rs` calls it
   after `apply_landlock`). No setuid helper or file capability is used.
3. **Output path.** Supervised staging is now the norm: acquisition writes
   into a `.partial-<uuid>` staging directory whose `.ofi-staging.json` marker
   records job id, pid, and start time, and `reap_stale_staging` reaps
   orphans after liveness and age checks.

The implementation and its test evidence are recorded in `phase8-review.md`
residual risk 1; the worker drops privilege to the dedicated account, and
`worker_drops_privilege` verifies the child's uid differs from the parent's.
