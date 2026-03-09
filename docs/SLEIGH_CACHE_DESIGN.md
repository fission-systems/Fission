# Sleigh XML/DOM 파싱 캐싱 설계 (2단계)

**목표**: `.sla` (및 `.pspec`, `.cspec`) 파싱 비용을 워커별 중복 제거. Read-Only 데이터를 전역 캐시하여 워커당 init 비용(~0.1–0.5s) 제거.

---

## 1. 현재 코드 흐름 분석

### 1.1 호출 체인

```
Architecture::init(store)
  → buildSpecFile(store)      // XML/.sla 경로 로드
  → restoreFromSpec(store)
      → buildTranslator(store) // new Sleigh(loader, context)
      → newtrans->initialize(store)  // ★ .sla 파싱 (무거운 작업)
```

### 1.2 buildSpecFile (sleigh_arch.cc:350-416)

```cpp
// 1) .pspec (프로세서 스펙) - XML 파싱
Document *doc = store.openDocument(processorfile);
store.registerTag(doc->getRoot());

// 2) .cspec (컴파일러 스펙) - XML 파싱
Document *doc = store.openDocument(compilerfile);
store.registerTag(doc->getRoot());

// 3) .sla 경로만 저장 (XML이 아님! 경로 문자열만)
istringstream s("<sleigh>" + slafile + "</sleigh>");
Document *doc = store.parseDocument(s);  // trivial: content = path
store.registerTag(doc->getRoot());
```

### 1.3 Sleigh::initialize (sleigh.cc:563-586)

```cpp
void Sleigh::initialize(DocumentStorage &store) {
  if (!isInitialized()) {
    const Element *el = store.getTag("sleigh");
    if (el == (const Element *)0)
      throw LowlevelError("Could not find sleigh tag");
    sla::FormatDecode decoder(this);
    ifstream s(el->getContent(), std::ios_base::binary);  // path 열기
    if (!s)
      throw LowlevelError("Could not open .sla file: " + el->getContent());
    decoder.ingestStream(s);   // ★ 바이너리 파싱 (압축 해제 + 디코딩)
    s.close();
    decode(decoder);            // ★ root, symtab 등 내부 테이블 구축
  }
  // ...
  discache = new DisassemblyCache(this, cache, ...);
}
```

**중요**: `.sla` 파일은 **XML이 아니라 바이너리** (`sla::FormatDecode`). `DocumentStorage`는 `.sla` 내용을 보관하지 않고 **경로만** 보관함.

---

## 2. 데이터 특성

| 항목 | 형식 | 크기/비용 | Read-Only |
|------|------|-----------|-----------|
| processorfile (.pspec) | XML | 소~중 | ✓ |
| compilerfile (.cspec) | XML | 소 | ✓ |
| slafile (.sla) | **바이너리** | 수 MB, 파싱 비용 큼 | ✓ (decode 결과) |
| Sleigh 내부 | - | root, symtab 등 | ✓ |
| ContextCache, DisassemblyCache | - | - | ✗ (가변) |

---

## 3. 캐싱 전략 옵션

### 옵션 A: DocumentStorage (processor + compiler) 캐시

- **대상**: `openDocument(processorfile)`, `openDocument(compilerfile)` 결과
- **구현**: 경로 → `Document*` (또는 `Element*`) 전역 맵. `DocumentStorage`가 doc을 소유하지 않도록 하거나, 캐시가 doc 생명주기 관리.
- **효과**: XML 파싱 2회 절약 (상대적으로 작음)

### 옵션 B: Sleigh Spec (decode 결과) 캐시 ★ 핵심

- **대상**: `Sleigh::decode(decoder)` 결과 — `root` (SubtableSymbol*), `symtab` (SymbolTable) 등
- **구현**: 
  1. `SleighSpec` 구조체에 read-only 파싱 결과 보관
  2. `slafile` 경로를 키로 전역 캐시
  3. `Sleigh::initializeFromSpec(shared_ptr<SleighSpec>)` 추가 — decode 생략, 기존 `cache`, `discache`만 생성
- **효과**: `.sla` 바이너리 파싱 완전 제거 (가장 무거운 부분)
- **난이도**: 높음 — `SleighBase`/`Sleigh`가 `root`, `symtab`을 직접 소유. 리팩터링 필요.

### 옵션 C: 하이브리드 — DocumentStorage + Sleigh path 레벨 캐시

- `DocumentStorage`를 채울 때: processor, compiler, sleigh(path)를 한 번에 로드
- `DocumentStorage` 자체를 `(archid, sleigh_id)` 키로 캐시
- `Architecture::init`에서 캐시된 `DocumentStorage`를 사용
- **주의**: `DocumentStorage`는 `doclist`를 소유하고 dtor에서 삭제. 캐시 시 생명주기 분리 필요.

---

## 4. 권장 접근: 단계적 구현

### Phase 2a: DocumentStorage (processor + compiler) 캐시

- `xml.cc`의 `DocumentStorage::openDocument`를 래핑하거나, `buildSpecFile` 진입 전에 processor/compiler `Document*`를 캐시에서 채움
- 기존 `store.openDocument` 호출을 `get_or_load_document(path)` 같은 캐시 함수로 대체
- **변경 범위**: `sleigh_arch.cc`의 `buildSpecFile` + 새 유틸리티

### Phase 2b: Sleigh decode 결과 캐시

- `SleighSpec` 또는 유사 구조로 decode 결과 분리
- `Sleigh::initialize(store)` 대신 `initializeFromCachedSpec(slafile)` 경로 추가
- `buildTranslator`에서: 캐시 히트 시 `new Sleigh(loader, context)` 후 `initializeFromCachedSpec` 호출
- **변경 범위**: `sleighbase.cc/hh`, `sleigh.cc/hh`, `sleigh_arch.cc`, `architecture.cc`

---

## 5. 참조 코드 위치

| 파일 | 함수/라인 | 역할 |
|------|-----------|------|
| `decompile/sleigh_arch.cc` | 350-416 `buildSpecFile` | processor/compiler/sleigh(path) 로드 |
| `decompile/sleigh_arch.cc` | 174-182 `buildTranslator` | `new Sleigh(loader, context)` 반환 |
| `decompile/sleigh.cc` | 563-586 `Sleigh::initialize` | .sla 열기, FormatDecode, decode() |
| `decompile/architecture.cc` | 625-630 `restoreFromSpec` | buildTranslator + initialize 호출 |
| `decompile/xml.cc` | 2452-2461 `openDocument` | XML 파일 → Document |
| `decompile/sleighbase.hh` | 64-65 | `root`, `symtab` (decode 결과) |

---

## 6. .cursorrules 준수

- **`ghidra_decompiler/decompile/`**: 업스트림 수정. `sleigh_arch.cc`는 FISSION 패치 적용됨.
- 변경은 `inject_sleigh.cc` 또는 Fission 래퍼(`src/`)로 한정하거나, `decompile/` 내 최소 변경으로 제한.
