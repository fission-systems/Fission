# SIGSEGV (exit 139) 디버깅 가이드

## 알려진 수정 (2026-03)

### 수정 1: TypePropagator (value_is_pointer) — 2026-03 업데이트

**원인:** `getHighTypeReadFacing`/`getHighTypeDefFacing`, `temp_type`, `base_type`, `fc->getOutputType()`가 dangling `Datatype*` 반환 → `getMetatype()` 호출 시 SEGV (ASAN 0xbebebebe).

**조치:**
- `value_is_pointer` 람다: 모든 Datatype 접근 제거 (inferred/temp/base/getOutputType 모두 SEGV 원인).
- Stack struct field 업데이트 루프: `existing->getMetatype()`, `field_type`(it->type) 접근 비활성화.
- **주의:** 1스레드에서도 SEGV 발생 — 추가 Datatype dangling 경로 존재 가능.

### Phase D 롤백 검증 결과 (2026-03)

**결과:** Object Pool 롤백 후에도 1스레드/8스레드 모두 exit 139 발생.

**해석 (경우 B):** Phase D Object Pool이 원인이 아님. Ghidra 엔진 구동 방식 어딘가에 **근본적인 UAF**가 잠복해 있음.

### UAF 수정: apply_inferred_types (2026-03)

**LLDB 크래시 위치:** `ghidra::Datatype::getMetatype()`, `write_back` 직후.

**원인:** `apply_inferred_types`에서 `temp_type`(vn->getTempType), `existing`(vn->getType), `ep->getPtrTo()` 등이 **dangling Datatype\*** → `getMetatype()` 호출 시 UAF.

**조치:**
- `temp_type` 블록 제거
- `existing`/`inferred` typeOrder 비교 제거 (getPtrTo()->getMetatype() UAF 방지)

**효과:** 1스레드 정상 동작(exit 0). 8스레드에서는 여전히 exit 139 — 추가 레이스/UAF 존재 가능.

### 8스레드 크래시 (2026-03 LLDB)

**크래시 위치:** `ghidra::Varnode::isWritten() const` (this=0x0, NULL 포인터 역참조)

```
* thread #2, stop reason = EXC_BAD_ACCESS (code=1, address=0x0)
    frame #0: libdecomp.dylib`ghidra::Varnode::isWritten() const + 12
```

**해석:** 1스레드 UAF(Datatype*)와 **다른** 버그. 워커 스레드(#2)에서 `vn->isWritten()` 호출 시 `vn`이 NULL.
- `op->getIn(slot)` 또는 `high->getInstance(i)` 등에서 null 반환 후 null 체크 없이 사용 가능
- 또는 cross-thread: 한 스레드의 Funcdata/Varnode가 다른 스레드에서 접근되며 이미 해제된 상태

**확인 사항:**
- `CachedCallbackSymbolProvider`: 워커별 DecompContext → 워커별 인스턴스이므로 공유 아님
- `SleighArchitecture::translators`, `description`: static 전역 — 다중 Architecture가 공유 가능

**권장 다음 액션:**
1. **전체 콜스택 확보:** `(lldb) thread backtrace all` 또는 `bt all`로 크래시 스레드의 frame #1..#10까지 확인 → NULL `vn`을 넘긴 caller 식별
2. **NULL 체크 보강:** Fission `src/` 내 `isWritten()` 호출부에 `vn` null 검사 추가 (TypePropagator, StackFrameAnalyzer, StructureAnalyzer 등)
3. **ASAN 8스레드 재현:** `FISSION_ASAN=1` 빌드 후 8스레드 실행 → ASAN이 정확한 caller 위치 보고

### 수정 4: UserPcodeOp::getOp() NULL 체크 (2026-03)

**LLDB 크래시 위치:** `ghidra::UserPcodeOp::getType() const` (this=0x0, address=0x28)

**원인:** `UserOpManage::getOp(index)`가 미등록 CALLOTHER 인덱스 시 NULL 반환 → null 체크 없이 `->getType()` 호출 시 SIGSEGV.

**조치:** 다음 호출부에 null 체크 추가:
- `jumptable.cc`: tmpOp null 시 `return false`
- `flow.cc`: CPUI_CALLOTHER 분기에서 userOp null 시 injectlist에 추가하지 않음
- `typeop.cc`: getInputLocal/getOutputLocal — userOp null 시 TypeOp fallback 반환
- `funcdata_block.cc`: userOp null 시 `return JumpTable::fail_callother`
- `printc.cc`: opCallother null 시 함수호출 형태 fallback; pushAnnotation null 시 size=0 유지
- `flow.cc`: injectUserOp null 시 early return

**효과:** UserPcodeOp NULL 역참조 크래시 제거. 8스레드에서 **추가** 크래시 존재(아래).

### 8스레드 크래시 2: Sleigh::resolve (2026-03 LLDB)

**크래시 위치:** `ghidra::Sleigh::resolve(ParserContext&) const` (address=0xdcf413c463be12e4, corrupt pointer)

```
* thread #4, stop reason = EXC_BAD_ACCESS (code=1, address=0xdcf413c463be12e4)
    frame #0: libdecomp.dylib`ghidra::Sleigh::resolve(ghidra::ParserContext&) const + 76
```

**해석:** `SleighArchitecture::translators` static 맵을 여러 Architecture가 공유. 8스레드 동시 P-Code 번역 시 Sleigh/ParserContext 상태가 꼬이거나 UAF로 corrupt pointer 역참조.

**후속 검토:** Sleigh 사용 직렬화(mutex) 또는 per-Architecture Sleigh 복제 검토.

### 수정 5: Sleigh::resolve / resolveHandles 직렬화 (2026-03)

**조치:** `sleigh.cc`에 전역 `sleigh_resolve_mutex` 추가. `Sleigh::resolve` 및 `Sleigh::resolveHandles` 본문 전체를 `std::lock_guard`로 감쌈.

**결과:** 8스레드 벤치마크에서 **크래시는 여전히 발생** (exit 139). 1스레드는 정상.
- Sleigh 락만으로는 해결되지 않음 → 추가 크래시 지점 존재 가능
- `Varnode::isWritten` NULL, `UserPcodeOp::getType` NULL 등 다양한 crash site가 보고됨
- ASAN 빌드로 8스레드 재현 시 정확한 현재 crash site 식별 권장

### 수정 2: decomp_create/destroy 직렬화 (DECOMP_FFI_LOCK)

**조치:** `DecompilerNative::new()` 및 `Drop`에서 전역 `DECOMP_FFI_LOCK` (Mutex)으로 `decomp_create`/`decomp_destroy` 호출 직렬화. (`wrapper.rs`)

**효과:** 생성·해제 시 Ghidra 전역 상태(Sleigh, TypeFactory) 접근 race 감소. 8스레드 시 대부분 성공하나 간헐적 139/138는 여전히 가능.

### 수정 3: 잔여 간헐적 크래시

- **증상:** create/destroy 직렬화 후에도 `RAYON_NUM_THREADS=8`에서 가끔 exit 138/139.
- **추정:** 분석 중 힙(Heap) 경합 또는 Ghidra 내부 전역 상태 접근 race.
- **우회:** `RAYON_NUM_THREADS=4` 이하 권장. Phase D (Object Pool) 도입 검토.

---

간헐적으로 발생하는 SIGSEGV는 멀티스레드 + C++ FFI 환경에서 흔한 이슈입니다. 이 문서는 원인 분석 및 해결을 위한 절차를 안내합니다.

---

## 1. 예상 원인

| 원인 | 설명 |
|------|------|
| **Teardown Race** | 여러 워커가 동시에 `DecompilerNative::drop` → `decomp_destroy` 호출 시 Ghidra 전역 상태 충돌 |
| **이중 해제 / UAF** | `DecompContext` 내부 또는 Sleigh/TypeFactory 캐시의 스레드-비안전 공유 자원 |
| **힙 락 경합** | 8개 스레드가 동시에 `new`/`delete` 호출 시 메모리 손상 가능성 |

---

## 2. ASAN(AddressSanitizer)으로 재현

### 2.1 C++ libdecomp ASAN 빌드

```bash
# 1. libdecomp를 ASAN으로 수동 빌드 (fission-ffi가 build 디렉터리를 먼저 기대함)
cd ghidra_decompiler/build
cmake -S .. -B . --fresh \
  -DCMAKE_CXX_FLAGS="-fsanitize=address -fno-omit-frame-pointer -g" \
  -DCMAKE_SHARED_LINKER_FLAGS="-fsanitize=address"
cmake --build . --target decomp -j8

# 2. RUSTFLAGS로 링크 경로 지정 후 Rust 빌드
cd ../..
RUSTFLAGS="-L $(pwd)/ghidra_decompiler/build" \
  cargo build -p fission-cli --features native_decomp --release
```

`FISSION_ASAN=1`로 `cargo build` 시 fission-analysis가 libdecomp를 ASAN으로 빌드하려 하나, fission-ffi가 먼저 링크를 시도해 실패할 수 있음. 위 순서로 수동 빌드 권장.

### 2.2 벤치마크 실행 (SIGSEGV 유도)

```bash
export DYLD_LIBRARY_PATH="$(pwd)/target/release:$DYLD_LIBRARY_PATH"
RAYON_NUM_THREADS=8 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp-all --benchmark --ghidra-compat --profile balanced --decomp-limit 100 \
  -o /tmp/asan_output.json
```

ASAN이 켜져 있으면 크래시 시 **정확한 파일·줄 번호와 콜스택**이 출력됩니다.

```bash
# ASAN 옵션으로 크래시 시 스택 출력 보장
export ASAN_OPTIONS="abort_on_error=1:halt_on_error=1:print_stacktrace=1"
export DYLD_LIBRARY_PATH="$(pwd)/target/release"   # macOS
RAYON_NUM_THREADS=8 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp-all --benchmark --ghidra-compat --profile balanced --decomp-limit 100
```

**참고:** 8스레드 크래시는 간헐적. decomp-limit 50에서는 성공, 100에서는 레이스에 따라 139 발생.

### 2.3 수동 CMake ASAN 빌드 (환경 변수 미지원 시)

```bash
cd ghidra_decompiler
rm -rf build && mkdir build && cd build

# ASAN 플래그로 Configure
cmake .. \
  -DCMAKE_CXX_FLAGS="-fsanitize=address -fno-omit-frame-pointer -g" \
  -DCMAKE_EXE_LINKER_FLAGS="-fsanitize=address"

# 빌드
cmake --build . --target decomp --parallel 4
```

이후 `cargo build` 시 fission-analysis build.rs가 기존 `ghidra_decompiler/build`를 사용하므로, 위에서 빌드한 ASAN libdecomp가 링크됩니다.

---

## 3. Core Dump + GDB/LLDB

### macOS (LLDB)

```bash
# 코어 덤프 허용
ulimit -c unlimited

# 벤치마크 실행 (SIGSEGV 발생 시 core 파일 생성)
RAYON_NUM_THREADS=8 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp-all --benchmark --decomp-limit 100 -o /tmp/out.json

# 크래시 후 분석
lldb -c core ./target/release/fission_cli
(lldb) bt          # 백트레이스
(lldb) thread list # 스레드 목록
(lldb) thread select 1
(lldb) bt          # 해당 스레드 백트레이스
```

### Linux (GDB)

```bash
ulimit -c unlimited
# 크래시 후
gdb ./target/release/fission_cli core
(gdb) bt
(gdb) info threads
```

---

## 4. 해결 방향 (원인별)

### 4.1 decomp_destroy 쪽 문제일 경우

- **증상:** 백트레이스에 `decomp_destroy`, `DecompContext` destructor 등이 포함
- **대응:** 워커 종료 시 `decomp_destroy`를 즉시 호출하지 않고, 메인 스레드에서 **순차적으로** 호출하도록 변경
- **구현 예:** 워커는 `DecompContext*`를 반환만 하고, 메인 스레드가 `JoinHandle` 완료 후 `decomp_destroy` 호출

### 4.2 Ghidra 전역 상태 충돌일 경우

- **증상:** Sleigh, TypeFactory, 전역 캐시 관련 심볼이 스택에 포함
- **대응:** Ghidra 내부 전역 초기화/해제에 락 추가, 또는 워커별 격리(예: 프로세스 분리) 검토

### 4.3 힙 락 경합/메모리 손상일 경우

- **대응:** Phase D Arena Allocator 도입으로 `new`/`delete` 호출 최소화

---

## 5. 재현 스크립트

```bash
./scripts/test/asan_benchmark.sh samples/windows/x64/putty.exe 100
```

위 스크립트는 `FISSION_ASAN=1`로 libdecomp를 빌드한 뒤 8스레드 벤치마크를 실행합니다. SIGSEGV 발생 시 ASAN이 크래시 위치를 출력합니다.

---

*최종 갱신: 2026-03*
