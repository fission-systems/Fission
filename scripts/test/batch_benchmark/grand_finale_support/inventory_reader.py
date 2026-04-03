#!/usr/bin/env python3
import sys
import json
from pathlib import Path

def analyze_inventory(report_dir):
    inventory_dir = Path(report_dir) / "inventory"
    if not inventory_dir.exists():
        print(f"오류: {inventory_dir} 디렉토리를 찾을 수 없습니다.")
        return

    total_if_no_exit = 0
    total_if_no_exit_accepted = 0
    total_structured = 0
    total_functions = 0

    print(f"'{inventory_dir}' 디렉토리 파싱 중...")
    
    for json_file in inventory_dir.glob("*.json"):
        with open(json_file, 'r', encoding='utf-8') as f:
            try:
                data = json.load(f)
                total_functions += 1
                
                # fission-automation inventory JSON 구조에서 통계 객체 추출 시도
                stats = data.get("build_stats", {})
                
                total_if_no_exit += stats.get("rule_block_if_no_exit_count", 0)
                total_if_no_exit_accepted += stats.get("rule_block_if_no_exit_accepted_count", 0)
                
                diagnosis = data.get("diagnosis", "")
                if diagnosis == "structured" or data.get("structured", False):
                    total_structured += 1
                    
            except json.JSONDecodeError:
                print(f"경고: 처리할 수 없는 JSON 파일 -> {json_file}")

    print("\n=== 인벤토리 분석 결과(Inventory Analysis Report) ===")
    print(f"분석된 총 함수 수: {total_functions}")
    print(f"구조화 완료 함수 수: {total_structured}")
    if total_functions > 0:
        print(f"구조화 달성률(Structured Ratio): {(total_structured / total_functions) * 100:.2f}%")
    
    print("\n--- Telemetry Metrics (If-NoExit 관련) ---")
    print(f"If-NoExit 발견 횟수: {total_if_no_exit}")
    print(f"If-NoExit 수용(Accepted) 횟수: {total_if_no_exit_accepted}")
    
    if total_if_no_exit > 0:
        acceptance_rate = (total_if_no_exit_accepted / total_if_no_exit) * 100
        print(f"수용률(Acceptance Rate): {acceptance_rate:.2f}%")
    else:
        print("수용률(Acceptance Rate): N/A (표본 없음)")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("사용법: python3 inventory_reader.py <fission_automation_결과_아티팩트_경로>")
        print("예시: python3 inventory_reader.py ../../../artifacts/fission-automation/177.../")
        sys.exit(1)
        
    analyze_inventory(sys.argv[1])
