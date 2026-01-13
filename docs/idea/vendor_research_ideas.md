# Vendor 프로젝트 기반 혁신 아이디어 (2026-01-13)

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
