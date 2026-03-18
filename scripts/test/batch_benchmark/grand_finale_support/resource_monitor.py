from __future__ import annotations

import os
import statistics
import subprocess
import threading
from typing import Any

try:
    import psutil

    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False


def _collect_process_resources(
    pid: int,
    interval_sec: float,
    result_holder: dict[str, Any],
) -> None:
    rss_list: list[float] = []
    cpu_list: list[float] = []
    try:
        proc = psutil.Process(pid)
        proc.cpu_percent()
        while True:
            try:
                if not proc.is_running():
                    break
            except psutil.NoSuchProcess:
                break
            try:
                rss_list.append(proc.memory_info().rss / (1024 * 1024))
                cpu_list.append(proc.cpu_percent(interval=interval_sec))
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                break
    except Exception:
        pass

    result_holder["max_rss_mb"] = round(max(rss_list), 2) if rss_list else 0.0
    result_holder["avg_rss_mb"] = round(statistics.fmean(rss_list), 2) if rss_list else 0.0
    result_holder["avg_cpu_pct"] = round(statistics.fmean(cpu_list), 2) if cpu_list else 0.0
    result_holder["max_cpu_pct"] = round(max(cpu_list), 2) if cpu_list else 0.0
    result_holder["sample_count"] = len(rss_list)


def run_popen_with_resource_monitor(
    popen: subprocess.Popen[Any],
    timeout_sec: float,
    interval_sec: float = 0.5,
) -> tuple[subprocess.CompletedProcess[Any], dict[str, Any]]:
    result_holder: dict[str, Any] = {}
    t = threading.Thread(
        target=_collect_process_resources,
        args=(popen.pid, interval_sec, result_holder),
        daemon=True,
    )
    t.start()
    try:
        returncode = popen.wait(timeout=timeout_sec)
        t.join(timeout=5.0)
        stdout = popen.stdout.read() if popen.stdout else ""
        stderr = popen.stderr.read() if popen.stderr else ""
        completed = subprocess.CompletedProcess(
            args=popen.args,
            returncode=returncode,
            stdout=stdout,
            stderr=stderr,
        )
        return completed, result_holder
    except subprocess.TimeoutExpired:
        t.join(timeout=1.0)
        popen.kill()
        try:
            stdout = popen.stdout.read() if popen.stdout else ""
            stderr = popen.stderr.read() if popen.stderr else ""
        except Exception:
            stdout, stderr = "", ""
        popen.wait(timeout=5)
        raise subprocess.TimeoutExpired(popen.args, timeout_sec, stdout, stderr)


def start_self_resource_monitor(
    interval_sec: float = 0.5,
) -> tuple[threading.Thread, dict[str, Any], threading.Event]:
    result_holder: dict[str, Any] = {}
    stop_event = threading.Event()

    def collect() -> None:
        rss_list: list[float] = []
        cpu_list: list[float] = []
        try:
            proc = psutil.Process(os.getpid())
            proc.cpu_percent()
            while not stop_event.is_set():
                try:
                    rss_list.append(proc.memory_info().rss / (1024 * 1024))
                    cpu_list.append(proc.cpu_percent(interval=interval_sec))
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    break
        except Exception:
            pass

        result_holder["max_rss_mb"] = round(max(rss_list), 2) if rss_list else 0.0
        result_holder["avg_rss_mb"] = round(statistics.fmean(rss_list), 2) if rss_list else 0.0
        result_holder["avg_cpu_pct"] = round(statistics.fmean(cpu_list), 2) if cpu_list else 0.0
        result_holder["max_cpu_pct"] = round(max(cpu_list), 2) if cpu_list else 0.0
        result_holder["sample_count"] = len(rss_list)

    t = threading.Thread(target=collect, daemon=True)
    t.start()
    return t, result_holder, stop_event
