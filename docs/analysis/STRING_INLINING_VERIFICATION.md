# 문자열 인라이닝 검증 결과 (2026-03)

## 검증 개요

`ReplaceStringPointersPass`가 실제 바이너리 디컴파일 결과에 제대로 적용되는지 `putty.exe`로 검증했습니다.

## 실행 환경

- **바이너리:** `samples/windows/x64/putty.exe`
- **명령:** `fission_cli ... --decomp-all --decomp-limit 50 --profile balanced -o /tmp/putty_decomp.json`
- **조치:** fission-cli에 `with_string_map(binary.inner().string_map.clone())` 전달 추가

## 검증 결과

### ✅ 성공적인 치환 예시

```c
InsertMenuA(param_1->field_ba0,0x10,0x10,lVar4,"S&pecial Command");
InsertMenuA(param_1->field_ba8,0x10,0x10,lVar4,"S&pecial Command");
```

- `InsertMenuA`의 마지막 인자로 `0x...` 주소 대신 `"S&pecial Command"` 문자열 리터럴이 들어감
- `&` (메뉴 액셀러레이터) 문자가 이스케이프 없이 올바르게 유지됨

### ✅ False Positive 회피

- `(char *)0x...` 및 `&DAT_0x1400...` 패턴: 출력에서 발견되지 않음
  - Ghidra 디컴파일러 또는 우리 패스에 의해 이미 치환된 것으로 보임

### ✅ 함수 주소 미치환 (의도 동작)

- `FUN_0x14000cf10`, `0x1400052b0` 등 함수 주소는 `string_map`에 없으므로 치환되지 않음
- `string_map`은 `.rdata`/`.rodata` 문자열만 포함

## 수정 사항 (검증 중 발견)

**CLI 경로에서 `string_map` 미전달 문제**

- `fission-cli`가 `PostProcessor`를 사용할 때 `with_string_map()`을 호출하지 않아
  문자열 치환 패스가 실제로 동작하지 않음
- **수정:** `run_sequential_decompilation`, `run_parallel_decompilation`, `decompile_and_output` 등
  모든 PostProcessor 생성处에 `.with_string_map(Some(binary.inner().string_map.clone()))` 추가

## 결론

문자열 인라이닝 패스는 putty.exe에서 의도대로 동작하며, CLI에 `string_map` 전달을 추가한 뒤
검증이 완료되었습니다. 실제 복잡한 바이너리에서 추가 패턴 누락이 발견되면 `strings.rs`의
정규식 및 패턴을 보완하면 됩니다.
