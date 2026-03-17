# Vendor-Inspired Research Ideas (2026-01-13)

> ⚠️ **Status: Exploratory, Not Active Roadmap**
> This document captures speculative research directions derived from third-party tools.
> It should be treated as research context, not as an active implementation plan.

`vendor` 디렉토리에 포함된 세계적인 오픈소스 보안 도구들의 핵심 메커니즘을 분석하여 Fission에 통합할 수 있는 고급 기능 아이디어입니다.

## 1. angr 기반의 심볼릭 실행 (Symbolic Execution) 통합

- **참고**: `vendor/angr-master`
- **아이디어**: 디컴파일러의 정적 분석이 해결하지 못하는 복잡한 경로를 심볼릭하게 해결.
- **세부 내용**:
  - **불투명 술어(Opaque Predicate) 제거**: `angr`의 solver를 사용하여 항상 참/거짓인 조건을 판별하고 해당 경로 제거.
  - **간접 점프(Indirect Jump) 해결**: 점프 테이블이 아닌 계산된 주소로의 점프를 심볼릭 실행을 통해 가능한 대상 주소 리스트 확보.

## 2. capa 기반의 기능(Capability) 자동 탐지 태깅

- **참고**: `vendor/capa-9.3.1`
- **아이디어**: 디컴파일된 코드의 의미를 분석하여 "이 함수는 AES 암호화를 수행함"과 같은 태그를 자동으로 부여.
- **세부 내용**:
  - `capa`의 규칙 엔진을 P-code 또는 리서치된 AST 레벨에 적용.
  - 단순 오프셋 기반 탐지를 넘어, 코드의 "행위"를 기준으로 함수 이름 추천 (예: `sub_401000` -> `encrypt_payload_aes`).

## 3. LIEF를 활용한 바이너리 변조 및 계측 (Instrumentation)

- **참고**: `vendor/LIEF-0.17.2`
- **아이디어**: 분석 중인 바이너리에 분석용 코드(Hook)를 삽입하거나 구조를 변경하여 동적 분석 지원.
- **세부 내용**:
  - **Import Injection**: 분석을 위해 특정 DLL/공유 라이브러리를 바이너리에 강제로 로드하도록 수정.
  - **Section Expansion**: 새로운 코드를 삽입할 공간을 확보하기 위해 바이너리 섹션 확장 및 재구성.

## 4. Detect-It-Easy (DIE) 기반의 시그니처 엔진 통합

- **참고**: `vendor/Detect-It-Easy-master`
- **아이디어**: 바이너리의 컴파일러, 패커, 인스톨러 정보를 정확히 파악하여 분석 설정 자동 최적화.
- **세부 내용**:
  - DIE의 `.db` 스크립트 엔진을 Fission Loader에 통합.
  - 패커가 감지될 경우 자동으로 Unpacker 작업 제안 또는 특정 컴파일러(Delphi, VB6 등) 전용 분석 모드 활성화.

## 5. Frida 기반의 정적-동적 하이브리드 디컴파일

- **참고**: `vendor/frida-main`
- **아이디어**: 실제 실행 중인 메모리 정보와 레지스터 값을 디컴파일 뷰에 실시간 매핑.
- **세부 내용**:
  - **Runtime Value Overlay**: 디컴파일된 코드의 변수 옆에 실제 실행 시의 값을 툴팁으로 표시.
  - **Dynamic Context-aware Decompilation**: 실행 시점에 결정되는 다형성(Polymorphism) 호출 대상을 실제 호출된 함수로 디컴파일 뷰에서 즉시 갱신.

## 6. RetDec 기반의 LLVM IR 변환 정교화

- **참고**: `vendor/retdec-5.0`
- **아이디어**: RetDec의 LLVM 기반 디컴파일 최적화 패스 도입.
- **세부 내용**:
  - **Instruction-to-IR Translation**: P-code를 더 표준적인 LLVM IR로 변환하여 LLVM의 방대한 최적화 생태계 활용.
  - **High-level Type Recovery**: RetDec의 타입 추론 엔진을 참고하여 C++ 클래스 및 상속 클래스 형태 복구 강화.

## 7. Kaitai Struct 기반의 복합 데이터 구조 분석

- **참고**: `vendor/kaitai_struct-master`
- **아이디어**: 선언적인 바이너리 포맷 정의를 사용하여 알 수 없는 데이터 구조체 자동 파싱.
- **세부 내용**:
  - 커스텀 파일 포맷이나 복잡한 네트워크 패킷 구조를 `.ksy` 파일로 정의하고, 이를 디컴파일러의 구조체 뷰와 연동.
  - 바이너리 내부에 포함된 리소스나 설정 파일 구조를 시각화.

## 8. x64dbg/ProcessHacker 스타일의 메모리 분석

- **참고**: `vendor/x64dbg-development`, `vendor/ProcessHacker-master`
- **아이디어**: 강력한 검색 및 시각화 기능을 갖춘 메모리 덤프 분석기 통합.
- **세부 내용**:
  - **Reference Follower**: 특정 메모리 주소나 상수를 참조하는 모든 코드 위치를 즉시 추적.
  - **String/Handle Search**: 로드된 프로세스 내의 모든 핸들, 문자열, 힙 객체를 스캔하여 코드 분석과 연결.

## 9. Radare2 기반의 ESIL 에뮬레이션 및 유연한 스크립팅

- **참고**: `vendor/radare2-master`
- **아이디어**: ESIL(Evaluable Strings Intermediate Language)을 활용하여 아키텍처 중립적인 가벼운 에뮬레이션 환경 구축.
- **세부 내용**:
  - **Virtual Machine Emulation**: 아키텍처에 상관없이 문자열 기반의 ESIL 가상 머신을 통해 특정 코드 블록의 사이드 이펙트(레지스터 변화 등)를 빠르게 계산.
  - **r2pipe 스타일 API**: 분석 프로세스의 모든 단계를 파이썬이나 자바스크립트로 제어할 수 있는 강력한 파이프 API를 제공하여 대규모 바이너리 자동 분석 지원.

## 10. Wireshark Dissector 연동을 통한 프로토콜 인지형 디컴파일

- **참고**: `vendor/wireshark-master`
- **아이디어**: 네트워크 통신 바이너리 분석 시, Wireshark의 수천 개의 프로토콜 디섹터 로직을 활용하여 데이터 구조 자동 파악.
- **세부 내용**:
  - **Socket Buffer Reconstruction**: `send`, `recv` 등의 시스템 콜에 사용되는 버퍼가 어떤 네트워크 프로토콜(HTTP, TLS, 커스텀 프로토콜 등)인지 식별하고 해당 구조체 템플릿을 디컴파일 뷰에 자동 적용.
  - **Field-to-Variable Mapping**: 패킷의 특정 필드(예: `Packet.Length`)를 사용하는 변수를 디컴파일러가 자동으로 인식하여 의미 있는 변수명으로 리네임.

## 11. Cheat Engine 스타일의 고성능 포인터 맵 및 시그니처 분석

- **참고**: `vendor/cheat-engine-master`
- **아이디어**: 변동하는 주소 공간에서도 유효한 데이터 접근 경로를 찾기 위해 고차원 포인터 분석 기능을 도입.
- **세부 내용**:
  - **Multi-level Pointer Map**: 전역 객체가 복잡한 포인터 체인을 통해 접근될 때, 수천 단계의 포인터 경로를 실시간으로 탐색하여 "이 데이터에 도달하는 모든 루트 주소"를 시각화.
  - **AOB (Array of Bytes) Signature Scan**: 특정 함수나 데이터 패턴을 찾는 CE 스타일의 와イルド카드 시그니처 스캐너를 통합하여 바이너리 버전 간 데이터 매핑 성능 향상.

## 12. Capstone 6.0 기반의 특수 명령어 로직 복구

- **참고**: `vendor/capstone-6.0.0-Alpha5`
- **아이디어**: 최신 CPU의 특수 명령어 및 가속기 명령어를 사용하는 고성능 연산 코드의 가독성 개선.
- **세부 내용**:
  - **Architecture-Specific Decomposition**: Intel AMX, ARM SVE/SME 등 하이엔드 연산 명령어를 사용하는 코드를 분석하여, 이를 단순히 인라인 어셈블리로 보여주는 대신 원래의 행렬 연산이나 벡터 연산 수식으로 복원.
  - **Instruction Detail 분석**: Capstone이 제공하는 상세 오퍼런드 정보를 활용하여, 부동 소수점 연산이나 비트 필드 조작 명령어를 보다 직관적인 C 연산자로 변환.

## 13. Binary Diffing 및 유사도 분석 엔진 (Radare2/RetDec 응용)

- **참고**: `vendor/radare2-master`, `vendor/retdec-5.0`
- **아이디어**: 패치된 바이너리나 서로 다른 컴파일러 옵션으로 빌드된 바이너리 간의 로직 차이를 시각적이고 구조적으로 분석.
- **세부 내용**:
  - **Control Flow Graph Diffing**: 두 바이너리의 함수 간 CFG 구조를 비교하여 추가되거나 삭제된 분기문을 하이라이트 표시.
  - **Symbolic Similarity Check**: 변수명이나 오프셋이 다르더라도 논리적으로 동일한 연산 흐름을 가진 함수를 찾아내어 기존 분석 정보를 전파(Porting).
