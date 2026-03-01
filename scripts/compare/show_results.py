#!/usr/bin/env python3
import json, glob

def show(label, directory, names, ci_avg=None, ci_count=None):
    files = sorted(glob.glob(directory + "/*.json"))
    print(f"\n=== {label} (Fission vs Ghidra) ===")
    total, count = 0, 0
    rows = []
    for f in files:
        d = json.load(open(f))
        ci = d["comparison_info"]
        sim = ci["similarity"]
        addr = None
        for k in names:
            if k in f:
                addr = k
                break
        name = names.get(addr, f.split("/")[-1])
        rows.append((name, sim))
        total += sim
        count += 1

    for name, sim in rows:
        bar = "█" * int(sim / 5) + "░" * (20 - int(sim / 5))
        flag = " ← x86 double 미지원" if name == "main" else ""
        print(f"  {name:24s} [{bar}] {sim:5.1f}%{flag}")

    # main 추정 (CI 결과 역산)
    if ci_avg and ci_count:
        main_sim = ci_avg * ci_count - total
        bar = "█" * int(main_sim / 5) + "░" * (20 - int(main_sim / 5))
        print(f"  {'main':24s} [{bar}] {main_sim:5.1f}% ← x86 double 미지원")
        total += main_sim
        count += 1

    print(f"  {'─'*57}")
    print(f"  {'종합 평균':24s}                       {total/count:5.1f}%")


x64_names = {
    "0x140001450": "add", "0x140001464": "multiply", "0x140001477": "print_message",
    "0x14000149d": "init_item", "0x14000156a": "create_item",
    "0x1400015e2": "sum_array",
}
x86_names = {
    "0x401460": "add", "0x401469": "multiply", "0x401473": "print_message",
    "0x40148e": "init_item", "0x40151c": "create_item",
    "0x401578": "calculate_discount", "0x4015fe": "sum_array",
}

show("x64 (MSVC 64bit)", "/Users/sjkim1127/Fission/scripts/result/20260225_010800_decomp-quality-v1", x64_names)
show("x86 (MinGW 32bit)", "/Users/sjkim1127/Fission/scripts/result/20260225_011012_decomp-quality-x86", x86_names, ci_avg=90.1, ci_count=8)

print("\n=== 세션 진행 이력 ===")
history = [
    ("x64 초기",    98.8, "─"),
    ("x86 초기",    80.0, "linear sweep 전: 337개 함수"),
    ("x86 linear sweep", 80.0, "20,062개 함수 발견, 베이스라인"),
    ("x86 Track 2/3/4", 80.0, "포인터 타입·배열 합성·헤더 제거"),
    ("x86 +normalize", 90.1, "FUNC callable 정규화 추가"),
    ("x64 최종",    98.8, "회귀 없음"),
]
for label, score, note in history:
    bar = "█" * int(score / 5) + "░" * (20 - int(score / 5))
    print(f"  {label:22s} [{bar}] {score:.1f}%  {note}")
