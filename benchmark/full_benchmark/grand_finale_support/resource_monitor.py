from __future__ import annotations

import os
import re
import statistics
import subprocess
import sys
import threading
import time
from typing import Any

try:
    import psutil

    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False


IS_MACOS = sys.platform == "darwin"


def _run_text_command(cmd: list[str], timeout_sec: float = 2.0) -> str:
    try:
        result = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=False,
        )
        return result.stdout.strip()
    except Exception:
        return ""


def _parse_uptime_load_avg(text: str) -> dict[str, float]:
    match = re.search(
        r"load averages?:\s*([0-9]+(?:\.[0-9]+)?)\s+([0-9]+(?:\.[0-9]+)?)\s+([0-9]+(?:\.[0-9]+)?)",
        text,
    )
    if not match:
        return {}
    return {
        "loadavg_1m": float(match.group(1)),
        "loadavg_5m": float(match.group(2)),
        "loadavg_15m": float(match.group(3)),
    }


def _parse_top_cpu(text: str) -> dict[str, float]:
    match = re.search(
        r"CPU usage:\s*([0-9]+(?:\.[0-9]+)?)%\s*user,\s*([0-9]+(?:\.[0-9]+)?)%\s*sys,\s*([0-9]+(?:\.[0-9]+)?)%\s*idle",
        text,
    )
    if not match:
        return {}
    return {
        "cpu_user_pct": float(match.group(1)),
        "cpu_sys_pct": float(match.group(2)),
        "cpu_idle_pct": float(match.group(3)),
    }


def _parse_vm_stat(text: str) -> dict[str, float]:
    if not text:
        return {}

    page_size = 4096
    size_match = re.search(r"page size of\s+([0-9]+)\s+bytes", text)
    if size_match:
        page_size = int(size_match.group(1))

    raw_pages: dict[str, int] = {}
    label_map = {
        "Pages free": "vm_pages_free",
        "Pages active": "vm_pages_active",
        "Pages inactive": "vm_pages_inactive",
        "Pages wired down": "vm_pages_wired",
        "Pages occupied by compressor": "vm_pages_compressor",
        "Pageins": "vm_pageins",
        "Pageouts": "vm_pageouts",
        "Swapins": "vm_swapins",
        "Swapouts": "vm_swapouts",
    }

    for line in text.splitlines():
        if ":" not in line:
            continue
        left, right = line.split(":", 1)
        key = left.strip()
        mapped = label_map.get(key)
        if not mapped:
            continue
        number_match = re.search(r"([0-9]+)", right.replace(".", ""))
        if not number_match:
            continue
        raw_pages[mapped] = int(number_match.group(1))

    if not raw_pages:
        return {}

    out: dict[str, float] = {}
    for key, value in raw_pages.items():
        out[key] = float(value)

    bytes_per_mb = 1024.0 * 1024.0
    for src, dst in (
        ("vm_pages_free", "vm_free_mb"),
        ("vm_pages_active", "vm_active_mb"),
        ("vm_pages_inactive", "vm_inactive_mb"),
        ("vm_pages_wired", "vm_wired_mb"),
        ("vm_pages_compressor", "vm_compressor_mb"),
    ):
        if src in out:
            out[dst] = round((out[src] * page_size) / bytes_per_mb, 2)

    return out


def collect_macos_activity_snapshot() -> dict[str, Any] | None:
    if not IS_MACOS:
        return None

    snapshot: dict[str, Any] = {
        "source": "macos_activity_monitor",
        "timestamp_epoch": round(time.time(), 3),
    }

    uptime_text = _run_text_command(["uptime"], timeout_sec=1.5)
    vm_stat_text = _run_text_command(["vm_stat"], timeout_sec=2.0)
    top_text = _run_text_command(["top", "-l", "1", "-n", "0"], timeout_sec=3.0)

    if uptime_text:
        snapshot.update(_parse_uptime_load_avg(uptime_text))
    if vm_stat_text:
        snapshot.update(_parse_vm_stat(vm_stat_text))
    if top_text:
        snapshot.update(_parse_top_cpu(top_text))

    if len(snapshot) <= 2:
        return None
    return snapshot


def summarize_macos_activity_delta(
    pre: dict[str, Any] | None,
    post: dict[str, Any] | None,
) -> dict[str, float] | None:
    if not pre or not post:
        return None

    delta: dict[str, float] = {}
    for key, pre_val in pre.items():
        post_val = post.get(key)
        if isinstance(pre_val, (int, float)) and isinstance(post_val, (int, float)):
            delta[key] = round(float(post_val) - float(pre_val), 3)

    if not delta:
        return None
    return delta


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
