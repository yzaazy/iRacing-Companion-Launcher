# One build at a time on this machine. Several sessions work in their own
# worktrees, each with its own target\, and parallel cargo builds and test runs
# starve the PC. A named mutex shared by every worktree makes them queue.
#
#   pwsh tools/build-lock.ps1 cargo build --release
#
# runs the command in the current directory while holding the lock and exits
# with its exit code. gate.ps1 dot-sources this file and holds the lock for the
# whole gate.

function Enter-BuildLock {
    $mutex = [System.Threading.Mutex]::new($false, 'Global\iRacingCompanionLauncher-build')
    try {
        if (-not $mutex.WaitOne(0)) {
            Write-Host '== waiting for the build lock (another worktree is building)'
            [void]$mutex.WaitOne()
        }
    } catch [System.Threading.AbandonedMutexException] {
        # The previous holder exited without releasing it; the lock is ours now.
    }
    $mutex
}

function Exit-BuildLock($mutex) {
    $mutex.ReleaseMutex()
    $mutex.Dispose()
}

if ($MyInvocation.InvocationName -ne '.') {
    if (-not $args) {
        Write-Host 'usage: pwsh tools/build-lock.ps1 <command> [args...]'
        exit 2
    }
    $lock = Enter-BuildLock
    try {
        $command, $rest = $args
        & $command @rest
        $code = $LASTEXITCODE
    } finally {
        Exit-BuildLock $lock
    }
    exit $code
}
