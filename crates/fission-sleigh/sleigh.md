# Ghidra SLEIGH 문서 전체 정리

> 대상 문서: Ghidra 공식 SLEIGH 문서(루트 페이지 + 이어지는 2~9장)
> 형식: 원문 전체의 **구조화 요약본 + 해설 확장판**
> 목적: SLEIGH 문법, 설계 개념, 작성 흐름, p-code 연산 표를 한 번에 참고할 수 있도록 재구성
> 독자 가정: Ghidra용 프로세서 모듈을 처음 만들거나, 기존 SLEIGH 명세를 읽어야 하는 역공학/리버싱 실무자

---

## 0. SLEIGH란 무엇인가

**SLEIGH**는 Ghidra에서 사용하는 **프로세서 명세 언어(processor specification language)**다. 이 언어의 가장 중요한 특징은 단순히 “바이트를 명령어 이름으로 바꾸는 포맷”이 아니라, **명령어의 비트 패턴, 사람이 읽는 어셈블리 표기, 그리고 의미론적 동작을 한 번에 기술한다는 점**이다.

목적은 크게 두 가지로 정리할 수 있다.

1. **디스어셈블리**: 기계어 비트 인코딩을 사람이 읽는 어셈블리 표현으로 바꾸기
2. **디컴파일 준비**: 각 명령어를 **p-code**로 번역해 데이터 흐름 분석과 디컴파일의 기반 만들기

문서 관점에서 SLEIGH는 단순한 문법이 아니라, 다음을 동시에 정의하는 언어다.

* 명령어 비트 패턴이 무엇인지
* 그 패턴이 어떤 오퍼랜드/레지스터/즉시값을 의미하는지
* 최종 디스어셈블리 문자열이 어떻게 보이는지
* 해당 명령어가 실제로 어떤 의미론적 동작을 하는지

즉, 일반적인 디스어셈블러 기술에서 흔히 분리되는 세 층이 SLEIGH에서는 한 체계 안에 묶인다.

* **인코딩 층**: 어떤 비트 조합이 어떤 명령어인가
* **표현 층**: 이를 어떤 어셈블리 문장으로 보여줄 것인가
* **의미론 층**: 이 명령어가 상태를 어떻게 바꾸는가

이 통합성 때문에 SLEIGH는 ISA 명세를 만들 때 매우 강력하지만, 동시에 각 섹션의 역할을 분명히 구분해서 써야 한다. display section에서 할 일과 semantic section에서 할 일을 혼동하면 명세가 빠르게 꼬인다.

### 역사적 배경

SLEIGH는 SLED(Specification Language for Encoding and Decoding)의 계보를 잇고, GHIDRA 역공학 요구사항에 맞춰 확장된 언어다. 핵심 확장은 **명령어 의미론을 p-code로 기술**할 수 있다는 점이다. 이로 인해 Ghidra는 디스어셈블 뿐 아니라 데이터 흐름 분석과 디컴파일까지 처리할 수 있다.

실무적으로는 이 차이가 매우 크다. 단순한 디스어셈블러 기술만으로는 다음과 같은 질문에 답하기 어렵다.

* 이 명령어가 어떤 레지스터를 읽고 쓰는가
* 이 조건 분기는 어떤 비교 결과에 의존하는가
* 이 메모리 접근의 유효 주소가 어떻게 계산되는가
* 이 함수가 어떤 값을 반환하는가

SLEIGH는 이런 질문에 답할 수 있도록, 명령어를 p-code라는 중간 표현으로 낮춘다. 따라서 잘 작성된 SLEIGH 명세는 단지 보기 좋은 어셈블리 출력용 파일이 아니라, Ghidra 분석 품질 전체를 좌우하는 핵심 컴포넌트다.

### SLEIGH를 볼 때의 관점

문서를 처음 읽을 때는 보통 문법 조각이 많아서 산만해 보이는데, 다음 관점으로 보면 구조가 정리된다.

* **define**: 세계관 만들기
* **token / field**: 비트 조각 이름 붙이기
* **attach**: 비트값에 의미 연결하기
* **table / constructor**: 명령어 문법 조립하기
* **disassembly action**: 표시용 계산하기
* **semantic section**: 실제 동작을 p-code로 쓰기
* **context**: 명령어 외부 상태를 디코딩에 반영하기

이 순서를 머릿속에 두면, SLEIGH 파일을 읽을 때 어디가 선언부이고 어디가 실제 번역 로직인지 쉽게 분리할 수 있다.

---

## 1. p-code 기초

SLEIGH의 핵심 산출물은 p-code다. p-code는 특정 CPU에 종속되지 않는 **RTL(Register Transfer Language)** 계열의 중간 표현이다. Ghidra는 개별 ISA의 수천 개 명령어를 직접 이해하는 대신, 각 명령어를 먼저 p-code로 바꾸고 그 위에서 공통 분석을 수행한다.

즉, SLEIGH의 quality는 곧 p-code의 quality다. 디스어셈블 문자열이 맞더라도 p-code가 잘못되면, 디컴파일 결과는 틀어지고 데이터 흐름 분석도 무너진다.

### p-code의 설계 목표

* 기계 독립적이어야 한다.
* 범용 프로세서를 모델링할 수 있어야 한다.
* 사용자 정의 레지스터와 주소 공간을 다룰 수 있어야 한다.
* 모든 데이터 변화는 명시적으로 드러나야 한다.
* 간접 부작용 없이, 입력/출력이 분명해야 한다.

여기서 특히 중요한 것은 **상태 변화가 명시적**이라는 점이다. p-code는 “어딘가에 영향이 있다”는 식의 암묵적 부작용을 싫어한다. 어떤 레지스터가 바뀌고 어떤 메모리가 갱신되는지, 가능한 한 직접 써야 한다. 이 특성 덕분에 SSA 변환, dead code 제거, 조건 추론 같은 분석이 쉬워진다.

### 1.1 Address Space

주소 공간은 p-code가 읽고 쓰는 메모리 모델이다.

* 보통 RAM 공간과 register 공간을 둔다.
* 각 공간은 이름과 주소 크기(size)로 정의된다.
* `size=4`이면 보통 32비트 주소를 뜻한다.
* `wordsize`를 지정하면 주소 1개가 가리키는 데이터 단위가 1바이트가 아닐 수 있다.

특수 공간도 있다.

* `unique`: 임시 저장용 공간
* `const`: 상수용 특수 공간

주소 공간은 단순한 분류 레이블이 아니다. Ghidra는 이 정보를 바탕으로 “이 값이 일반 메모리인지, 레지스터인지, 임시값인지”를 구분한다. 따라서 공간 설계는 분석기와의 계약에 가깝다.

예를 들어 `register_space`는 일반 메모리와 달리 포인터를 통해 간접 접근하는 공간이 아니라고 가정된다. 반면 `ram_space`는 LOAD/STORE의 대상이 될 수 있다. 이 차이를 잘못 모델링하면 alias 분석이 이상해질 수 있다.

### 1.2 Varnode

**varnode**는 p-code가 다루는 데이터의 최소 단위다.

* 어떤 주소 공간에 속하는 연속 바이트 구간
* 특징은 `시작 주소`와 `크기(바이트 수)`
* 자체적으로는 타입이 없다
* 연산이 수행될 때 정수/부동소수/불리언처럼 해석된다

불리언은 1바이트이며, 일반적으로 0은 false, 1은 true로 본다.

varnode를 이해할 때 핵심은 “타입보다 저장 위치와 폭이 우선”이라는 점이다. C 언어의 `int`, `short`, `pointer`처럼 타입 중심으로 생각하면 헷갈릴 수 있다. p-code는 먼저 바이트 구간을 정의하고, 그 바이트 구간이 어떤 연산에 들어가느냐에 따라 의미가 정해진다.

예를 들어 같은 4바이트 varnode라도:

* `INT_ADD`에 들어가면 정수
* `FLOAT_ADD`에 들어가면 부동소수
* `BRANCH`의 대상이 되면 주소 성격

으로 취급된다.

### 1.3 Operations

각 기계어 명령어는 여러 개의 p-code 연산으로 번역된다.

* 연산은 입력 varnode 여러 개를 받을 수 있다.
* 출력은 최대 1개다.
* 출력에 대해서만 상태 변화가 일어난다.
* 따라서 모든 데이터 변화가 명시적이다.

이 규칙 때문에 p-code는 데이터 흐름을 추적하기 쉽다. 예를 들어 `dst = src1 + src2`는 결국 `INT_ADD` 하나나, 필요한 경우 임시 varnode를 거친 여러 연산으로 표현된다. CPU가 내부적으로 플래그를 갱신하더라도, SLEIGH에서는 그 플래그 갱신도 각각 별도 문장으로 써 주어야 한다.

### p-code 연산 범주

* 데이터 이동: `COPY`, `LOAD`, `STORE`
* 산술: `INT_ADD`, `INT_SUB`, `INT_MULT`, `INT_DIV` 등
* 논리: `INT_AND`, `INT_OR`, `INT_XOR`, `INT_LEFT`, `INT_RIGHT` 등
* 비교: `INT_EQUAL`, `INT_LESS`, `INT_SLESS` 등
* 불리언: `BOOL_AND`, `BOOL_OR`, `BOOL_XOR`, `BOOL_NEGATE`
* 부동소수: `FLOAT_ADD`, `FLOAT_DIV`, `FLOAT_SQRT` 등
* 분기: `BRANCH`, `CBRANCH`, `CALL`, `RETURN` 등
* 확장/절단: `INT_ZEXT`, `INT_SEXT`, `PIECE`, `SUBPIECE`
* 관리 코드: `CPOOLREF`, `NEW`

실무에서는 모든 연산을 암기할 필요는 없지만, 최소한 다음 4부류는 익숙해져야 한다.

1. **정수 산술/비교**: 대부분의 일반 명령어 번역에 필수
2. **LOAD/STORE**: 메모리 오퍼랜드를 다룰 때 필수
3. **PIECE/SUBPIECE, ZEXT/SEXT**: 크기 변환 시 필수
4. **BRANCH/CBRANCH/CALL/RETURN**: 제어 흐름 모델링에 필수

---

## 2. 기본 파일 구조

SLEIGH 명세는 보통 하나의 파일에 작성되지만, `@include`로 분할 가능하다. 대형 ISA에서는 하나의 거대한 파일보다, 공통 정의·레지스터 정의·오퍼랜드 정의·명령어 그룹 정의로 분할하는 편이 관리하기 쉽다.

### 2.1 주석

* `#`부터 줄 끝까지
* 단, constructor의 display section에서는 `#`가 출력 문자로 취급될 수 있다

이 예외가 중요한 이유는, SLEIGH가 순수 프로그래밍 언어가 아니라 디스어셈블리 출력 문자열을 함께 다루기 때문이다. 즉, 같은 기호가 어떤 문맥에서는 주석 시작이고 다른 문맥에서는 출력용 텍스트일 수 있다.

### 2.2 식별자

허용 문자:

* 영문 대소문자
* 숫자
* `.`
* `_`

제약:

* 숫자로 시작하면 안 된다

실무적으로는 식별자 네이밍 규칙을 미리 정하는 것이 좋다. 예를 들면:

* 토큰 필드: `op`, `subop`, `imm8`, `mode`
* 테이블: `reg32`, `addrmode`, `branchdest`
* context 변수: `thumb`, `isa_mode`, `bank`
* 임시적인 의미 표기: `cc`, `sz`, `ext`

명명 규칙이 없으면, field와 table, local operand가 같은 이름을 공유하면서 명세 가독성이 급격히 떨어진다.

### 2.3 문자열

* 큰따옴표 `"..."` 사용
* 내부 문자는 특수 의미를 잃고 리터럴로 처리된다

display section에서 문자열 리터럴은 매우 자주 사용된다. 특히 쉼표, 괄호, 접두사 문자(`#`, `$`, `%`)를 안정적으로 넣고 싶다면 문자열로 명시하는 편이 명확하다.

### 2.4 정수

지원 표기:

* 10진수: `123`
* 16진수: `0x7B`
* 2진수: `0b1111011`

일반적으로 unsigned로 취급되지만, 패턴 문맥에서는 signed처럼 다뤄질 수 있다.

여기서 헷갈리기 쉬운 지점은 “문자열로 출력되는 값”과 “semantic section에서의 연산 값”이 다를 수 있다는 점이다. 필드가 signed 속성을 갖거나 attach를 통해 다른 값 집합으로 치환되면, 화면에 보이는 수와 내부 해석이 달라질 수 있다.

### 2.5 공백

공백/탭/개행 등은 대부분 파싱 결과에 영향이 없다. 단, 문자열 리터럴 내부는 예외다.

다만 공백이 의미를 바꾸지 않는다고 해서 아무렇게나 쓰는 것은 좋지 않다. SLEIGH는 한 줄에 많은 정보를 담는 일이 많아서, 줄바꿈과 들여쓰기를 규칙적으로 맞춰야 디버깅이 쉬워진다. 특히 constructor는 display, pattern, action, semantic이 한 덩어리이므로 시각적 구분이 중요하다.

---

## 3. 전처리기

전처리 지시문은 **줄 맨 앞의 `@`** 로 시작해야 한다. 문법 자체는 단순한 편이지만, ISA 변종 지원이나 빌드 타깃 분기에서 매우 유용하다.

### 3.1 파일 포함

```sleigh
@include "other.slaspec"
```

* 지정 파일 내용을 현재 위치에 삽입한 것처럼 처리
* 중첩 include 가능

이 기능은 대형 프로세서 패밀리에서 특히 중요하다. 예를 들어:

* 공통 레지스터 정의
* 공통 addressing mode 정의
* 코어별 확장 opcode
* FPU / DSP / SIMD 확장 세트

를 별도 파일로 분리할 수 있다.

### 3.2 매크로

```sleigh
@define ENDIAN "big"
define endian=$(ENDIAN);
```

* 파라미터 없는 단순 치환 매크로
* 확장 구문: `$(NAME)`
* 제거: `@undef NAME`

C 전처리기처럼 복잡한 매크로 시스템은 아니지만, 동일 명세를 여러 설정으로 재사용할 때 충분히 유용하다. 예를 들어 big/little variant, feature flag, 실험용 옵션 분기 등에 쓸 수 있다.

### 3.3 조건부 컴파일

지원 구문:

* `@ifdef`
* `@ifndef`
* `@if`
* `@elif`
* `@else`
* `@endif`

특징:

* 중첩 가능
* `defined(NAME)` 사용 가능
* `&&`, `||`, `^^` 사용 가능
* 문자열 비교는 동등/부등 중심

실무에서는 이 기능을 남발하지 않는 것이 좋다. 조건부가 깊어질수록 어떤 변형에서 어떤 constructor가 실제로 살아남는지 파악하기 어려워진다. 되도록이면 다음 기준을 권장할 수 있다.

* **전역 특성 분기**: 전처리기 사용
* **디코딩 중 런타임 상태 분기**: context 사용
* **단순 반복 제거**: p-code macro 또는 include 사용

즉, 빌드 시점과 디스어셈블 시점 문제를 섞지 않는 것이 좋다.

---

## 4. 기본 정의(define)

SLEIGH 파일은 먼저 필요한 정의부터 선언한다. 이 부분은 일종의 “타깃 머신의 물리적 세계관”을 세팅하는 단계다.

### 4.1 엔디언

가장 먼저 와야 한다.

```sleigh
define endian=big;
```

또는

```sleigh
define endian=little;
```

영향 범위:

* 정수 해석
* 토큰 필드 비트 번호 해석
* 겹치는 레지스터 정의

엔디언은 단순히 메모리 로드 순서만 바꾸는 옵션이 아니다. **토큰 비트 번호가 어떤 바이트 배열을 기준으로 해석되는지**에도 영향을 주므로, 잘못 잡으면 필드 추출이 전부 어긋날 수 있다. 따라서 토큰 정의 전에 반드시 architecture 문서를 기준으로 비트 번호 체계를 점검해야 한다.

### 4.2 정렬

```sleigh
define alignment=4;
```

* 명령어 정렬 단위 지정
* 미정렬 주소의 명령어를 오류로 볼 수 있다

고정 길이 ISA에서는 매우 자연스럽지만, variable-length ISA에서도 최소 정렬 단위를 명시하는 것이 디스어셈블 품질에 도움을 줄 수 있다. 다만 실제 CPU가 바이트 정렬 명령어를 허용한다면 과도한 alignment는 오히려 false negative를 만든다.

### 4.3 주소 공간

기본 형식:

```sleigh
define space ram type=ram_space size=4 default;
define space register type=register_space size=4;
```

속성:

* `type=(ram_space|rom_space|register_space)`
* `size=정수`
* `default`
* `wordsize=정수`

핵심 차이:

* `ram_space`: 읽기/쓰기 가능, 일반 메모리
* `register_space`: 포인터 간접 접근이 없다고 가정되는 레지스터 공간
* `rom_space`: 쓰기 불가 RAM 유사 공간

`default` 공간은 명시적 공간 이름 없이 쓰는 주소가 기본적으로 속하는 공간이다. 대부분 일반 메모리 공간에 붙인다. 여러 공간을 쓰는 ISA에서는 “주소 계산 결과가 어느 공간으로 가는지”를 일관되게 모델링하는 것이 중요하다.

또한 Harvard 구조나 I/O space가 분리된 아키텍처에서는 주소 공간을 더 세분할 수 있다. 이때는 단순히 CPU 설명서의 메모리 맵을 옮기는 것이 아니라, Ghidra 분석이 어떤 distinction을 필요로 하는지 기준으로 설계하는 편이 낫다.

### 4.4 레지스터 이름 붙이기

```sleigh
define register offset=0 size=4 [ r0 r1 r2 r3 ];
```

* 특정 공간의 특정 offset에 연속 varnode를 만든다
* 동일 offset 재사용으로 겹치는 레지스터 모델링 가능
* `_` 는 슬롯 건너뛰기

예: x86의 `EAX / AX / AL` 같은 중첩 구조 모델링 가능

이 기능을 이용하면 뱅크드 레지스터, 부분 레지스터, alias 레지스터를 표현할 수 있다. 단, 겹침이 많을수록 semantic section에서 크기와 부분 추출을 더 신중히 다뤄야 한다.

예를 들어 32비트 레지스터와 16비트 하위 절반, 8비트 하위 바이트를 동시에 정의하면, 어떤 명령어가 어느 폭을 읽고 쓰는지 정확히 기술해야 디컴파일 결과가 안정된다.

### 4.5 비트 범위 레지스터

```sleigh
define bitrange zf=statusreg[10,1] cf=statusreg[11,1];
```

* 상태 레지스터 내부 플래그 같은 비트 단위 레지스터 표현용
* 실제 p-code는 바이트 단위만 직접 다루므로 내부적으로 여러 연산으로 확장된다
* 바이트 경계에 맞으면 일반 varnode처럼 동작할 수도 있다

플래그 비트가 많은 ISA에서는 편리하지만, 너무 남발하면 p-code가 과도하게 장황해질 수 있다. 실무적으로는 다음 기준이 유용하다.

* 분석에 자주 쓰이는 플래그: bitrange로 노출
* 드물게만 쓰이고 묶음으로 처리 가능한 플래그: 전체 status register 중심으로 모델링

### 4.6 사용자 정의 p-code 연산

```sleigh
define pcodeop arctan;
```

* 의미를 Ghidra가 해석할 수 없는 **블랙박스 연산** 예약
* 입력/출력 데이터 흐름은 유지되지만 정교한 분석은 어려워짐
* 정말 필요한 경우에만 사용 권장

이 기능은 하드웨어 특수 동작, 코프로세서, 암호 엔진, VM 하이퍼콜, 구현 난도가 매우 높은 연산에 대해 임시 안전판으로 쓸 수 있다. 하지만 너무 많은 동작을 user-defined op로 밀어 넣으면, 디컴파일러는 그 지점을 거의 이해하지 못한다. 따라서 가능한 한 표준 p-code로 풀어 쓰고, 마지막 수단으로만 사용하는 것이 바람직하다.

---

## 5. 심볼 시스템

SLEIGH에서 거의 모든 것은 **심볼(symbol)** 중심으로 조립된다. 이 개념은 문법과 의미론이 결합되는 핵심이므로, 한 번 정확히 잡아 두는 것이 좋다.

### 5.1 Specific Symbol

Specific Symbol은 다음을 가진다.

1. 디스어셈블리에서 출력될 문자열
2. 의미론에서 사용할 varnode 및 그 생성 방법

예: 이미 정의된 레지스터 이름

쉽게 말해 “지금 이 명령어 맥락에서 실제로 선택된 구체적 대상”이다. 예를 들어 필드 값이 2라서 `r2`가 선택되었다면, 그 순간 `r2`는 specific symbol이다.

### 5.2 Family Symbol

Family Symbol은 **명령어 인코딩 → specific symbol** 의 매핑이다.

즉, 같은 이름의 심볼이 문맥(현재 명령어 비트 패턴)에 따라 다른 specific symbol로 해석될 수 있다.

예를 들어 `reg`라는 family symbol이 있다고 하면, 디코딩 결과에 따라 `r0`, `r1`, `r2`, `r3` 중 하나를 내놓을 수 있다. 이 구조 덕분에 SLEIGH는 “비트 필드 값에 따라 달라지는 오퍼랜드”를 우아하게 모델링한다.

### 5.3 Table

복잡한 심볼은 **table** 로 구축한다.

* 여러 constructor를 묶어 하나의 family symbol을 만든다
* 최종 루트 테이블 이름은 `instruction`

table은 일종의 비결정적 문법 규칙 집합처럼 볼 수 있다. 각 constructor가 하나의 가능한 케이스이고, 패턴 제약에 따라 그중 하나가 선택된다.

### 5.4 네임스페이스

거의 모든 전역 식별자는 하나의 전역 스코프를 공유한다.

중복되면 안 되는 대표 항목:

* 주소 공간 이름
* 토큰/필드 이름
* user-defined p-code op 이름
* 레지스터 이름
* 매크로 이름
* 테이블 이름

각 constructor는 **오퍼랜드용 로컬 스코프**를 만든다.

이 구조 때문에 이름 충돌을 피하는 습관이 중요하다. 특히 field 이름과 table 이름을 너무 일반적으로 잡으면, constructor 내부에서 무엇이 전역 심볼이고 무엇이 로컬 operand인지 한눈에 보기 어려워진다.

### 5.5 미리 정의된 심볼

* `instruction`: 루트 테이블
* `const`: 상수 공간
* `unique`: 임시 공간
* `inst_start`: 현재 명령어 주소
* `inst_next`: 다음 명령어 주소
* `inst_next2`: 그다음 명령어 주소
* `epsilon`: 빈 패턴

`inst_start`, `inst_next`, `inst_next2`는 특히 제어 흐름 명세에서 자주 등장한다. 상대 분기나 skip-next 스타일 ISA를 다룰 때 사실상 필수다.

---

## 6. 토큰과 필드

### 6.1 Token 정의

토큰은 명령어를 이루는 바이트 단위 조각이다. SLEIGH는 먼저 기계어 스트림을 토큰 단위로 보고, 그 토큰 안의 필드를 통해 의미를 꺼낸다.

```sleigh
define token instr(16)
  opcode=(10,15)
  r1=(6,8)
  r2=(3,5)
;
```

핵심:

* 토큰 크기는 8의 배수 비트여야 한다
* 필드는 토큰 안의 비트 범위다
* 필드 크기는 8의 배수일 필요 없다
* 다중 바이트 토큰은 엔디언 영향을 받는다
* 토큰 단위 엔디언 override 가능

필드 속성:

* `signed`
* `hex`
* `dec` (문서상 지원 언급이 있으나 일부 구현 제약 있음)

토큰은 “실제 CPU 문서에 나오는 인코딩 박스”와 가장 가까운 개념이다. 따라서 ISA 매뉴얼에서 opcode map과 operand bit layout을 옮길 때 보통 이 섹션부터 설계하게 된다.

다만 토큰을 꼭 실제 fetch 단위와 일치시킬 필요는 없다. 중요한 것은 SLEIGH가 안정적으로 패턴을 구분할 수 있는 구조를 만드는 것이다. 어떤 경우에는 한 명령어를 16비트 토큰 2개로 쪼개는 것이 낫고, 어떤 경우에는 32비트 토큰 1개로 두는 것이 낫다.

### 6.2 필드는 가장 기본적인 family symbol

필드는 기본적으로 다음 해석을 갖는다.

* 표시: 해당 비트를 정수로 해석한 문자열
* 의미론: 해당 정수를 나타내는 constant varnode

즉, attach를 하지 않은 필드는 가장 원시적인 의미의 operand다. 화면에도 숫자로 보이고, semantic에서도 상수처럼 동작한다.

### 6.3 attach로 필드 의미 바꾸기

#### 6.3.1 attach variables

필드를 레지스터 선택자로 바꾼다.

```sleigh
attach variables [ r1 r2 ] [ eax ecx edx ebx ];
```

* 필드 값이 리스트 index 역할
* 해당 index의 레지스터가 display와 semantic 둘 다 결정
* `_` 또는 리스트 길이 부족은 invalid encoding 처리

이것은 실전에서 매우 자주 쓰인다. 레지스터 번호 필드는 대부분 attach variables 대상이다.

#### 6.3.2 attach values

필드가 다른 정수 집합을 뜻하게 만든다.

```sleigh
attach values scale [ 1 2 4 8 ];
```

예를 들어 x86 SIB scale처럼 “인코딩 값 0,1,2,3이 실제 의미 1,2,4,8”로 바뀌는 경우에 딱 맞는다.

#### 6.3.3 attach names

표시 문자열만 바꾸고 의미는 원래 정수 해석을 유지한다.

```sleigh
attach names cond [ "eq" "ne" "lt" "gt" ];
```

이는 조건 접미사나 축약 표기처럼, 디스어셈블리 표현만 바꾸고 semantic은 별도로 처리하고 싶을 때 유용하다.

### 6.4 Context Variable

문맥 변수는 토큰이 아니라 **레지스터 위에 정의된 필드**다.

```sleigh
define context statusreg
  mode=(3,3)
;
```

* 디코딩이 현재 프로세서 상태에 의존할 때 사용
* 일반 필드처럼 패턴에서 사용 가능
* `noflow` 속성을 붙이면 global context 변화가 1개 명령에만 영향을 주게 만들 수 있다

context는 “현재 읽는 바이트만으로는 해석이 불가능한 경우”를 해결하는 도구다. 예를 들어 같은 opcode가 ARM mode와 Thumb mode에서 다르게 해석되거나, 이전 prefix가 다음 명령의 오퍼랜드 해석을 바꾸는 경우에 필요하다.

---

## 7. Constructor

constructor는 SLEIGH의 핵심 조립 단위다.

* 기존 심볼을 이용해 새 family symbol의 한 케이스를 정의한다
* table에 속한다
* 실제 문법/디스어셈블리/의미론을 한꺼번에 기술한다

실무적으로는 “한 constructor = 하나의 해석 가능한 패턴 케이스”라고 생각하면 된다. 어떤 경우에는 실제 ISA 명령어 하나가 constructor 하나에 대응하고, 어떤 경우에는 pseudo-instruction이나 addressing mode 조각이 constructor 하나가 된다.

### 7.1 constructor의 5개 섹션

순서는 항상 다음과 같다.

1. Table Header
2. Display Section
3. Bit Pattern Section
4. Disassembly Actions Section
5. Semantic Section

이 5개를 역할별로 다시 정리하면 다음과 같다.

* **Table Header**: 이 규칙이 어느 테이블에 속하는가
* **Display Section**: 화면에 무엇을 어떻게 출력할 것인가
* **Bit Pattern Section**: 어떤 인코딩 비트가 이 규칙을 선택하는가
* **Disassembly Actions**: 표시를 위해 무엇을 계산할 것인가
* **Semantic Section**: 실제 상태 변화는 무엇인가

이 구분을 머릿속에 두면 constructor를 읽을 때 훨씬 빨라진다.

---

### 7.2 Table Header

```sleigh
mode1: ...
```

* `mode1` 테이블에 속한 constructor
* 처음 등장하면 새 table 생성
* 이미 있으면 같은 table에 constructor 추가
* 루트 테이블은 이름 없이 `:` 로 시작

```sleigh
: ...
```

루트 `instruction` table에 직접 속하는 constructor는 실제 명령어 엔트리 포인트가 된다. 반면 보조 table은 주로 operand나 addressing mode를 추상화하는 데 쓴다.

좋은 SLEIGH 설계는 대개 모든 것을 루트 constructor에 우겨 넣지 않고, operand와 address calculation을 하위 table로 적절히 분리한다.

---

### 7.3 Display Section

table header 뒤에서 `is` 전까지가 display section이다.

역할:

* 최종 디스어셈블리 문자열 구성
* 로컬 오퍼랜드 식별자 선언

규칙 요약:

* 식별자는 그냥 문자로 출력되지 않고, 보통 로컬 심볼로 간주된다
* 큰따옴표로 감싸면 리터럴
* 공백은 정규화된다
* `^` 는 심볼과 리터럴을 공백 없이 붙이는 용도

display section은 “예쁘게 출력하는 곳”이지만, 동시에 operand 이름을 선언하는 장소이기도 하다. 그래서 단순 문자열 템플릿처럼 보이지만, 사실은 문법 조립의 일부다.

#### mnemonic

루트 constructor에서는 display section 첫 토큰이 기본적으로 mnemonic이다.

```sleigh
:and r1,r2 is ...
```

여기서 `and`는 mnemonic이고 오퍼랜드가 아니다.

#### `^` 연산자

```sleigh
:bra^cc op1 is ...
```

* `bra`와 `cc`의 display를 공백 없이 결합
* 조건 접미어가 붙는 명령어 표현에 유용

실무 예시는 다음과 같은 경우다.

* `b.eq`, `b.ne`
* `mov.w`
* `adds`, `subs`

즉, mnemonic이 고정 문자열 하나가 아니라 여러 심볼 조합으로 구성되는 ISA에서 유용하다.

#### display section 설계 팁

* 리터럴과 심볼을 의도적으로 구분해 써라.
* 보이는 문자열만 맞추려 하지 말고, operand 구조가 읽히게 설계하라.
* addressing mode 표현은 하위 table로 분리하는 편이 재사용성이 좋다.

예를 들어 메모리 오퍼랜드가 `[base + index*scale + disp]` 구조라면, 이를 display section에서 매번 조립하기보다 별도 table로 두는 편이 훨씬 관리하기 쉽다.

---

### 7.4 Bit Pattern Section

`is` 다음부터 `[` 또는 `{` 전까지가 패턴 섹션이다. 이 부분은 어떤 인코딩이 해당 constructor를 선택하는지를 정의한다.

#### 7.4.1 기본 제약(constraint)

```sleigh
:halt is opcode=0x15 { ... }
```

* 특정 필드가 특정 값이어야 한다는 조건
* 가장 흔한 형태

#### 7.4.2 `&` 와 `|`

* `&`: 둘 다 만족
* `|`: 둘 중 하나 만족

가능하면 `&` 위주가 효율적이다. `|`는 구현상 더 많은 상태 분기가 필요할 수 있다.

`|`를 많이 쓰는 명세는 종종 “인코딩 체계가 아직 정리되지 않았다”는 신호이기도 하다. 때로는 별도 constructor로 쪼개는 편이 더 명확하다.

#### 7.4.3 오퍼랜드 정의 및 subtable 호출

display section에서 선언한 로컬 식별자를 bit pattern에서 단독으로 쓰면, 같은 이름의 전역 family symbol과 연결된다.

```sleigh
:add r1,r2 is opcode=7 & r1 & r2 { ... }
```

의미:

* `opcode`는 고정 제약
* `r1`, `r2`는 해당 필드/테이블이 결정하는 심볼을 끌어온다

이 패턴을 이해하면, SLEIGH가 parser combinator처럼 operand를 재귀적으로 해석한다는 점이 보인다.

#### 7.4.4 가변 길이 명령어

여러 토큰을 조합할 수 있다.

##### `;` 연산자

토큰의 **순서**를 지정한다.

```sleigh
:add reg,imm16 is op=3 & reg; imm16 { ... }
```

##### `...` 연산자

길이가 다른 패턴끼리 결합할 때 길이를 맞춰준다.

* 좌/우 어느 쪽에 붙느냐에 따라 정렬 기준이 달라진다
* variable-length operand와 기본 opcode 조건을 합칠 때 유용

이 기능은 prefix 명령어나 확장 immediate 형식이 있는 ISA에서 자주 필요하다. 고정 길이 명령어에 익숙하면 낯설 수 있지만, 실제로는 매우 강력한 도구다.

#### 7.4.5 invisible operand

패턴에는 필요하지만 디스어셈블리에 출력하지 않는 오퍼랜드를 만들 수 있다.

대표 예:

* 상대 분기 대상 계산용 심볼
* 내부적으로만 쓰는 extension word
* display에는 숨기고 semantic에서만 쓰는 mode selector

#### 7.4.6 empty pattern

* `epsilon`은 모든 것을 매치하는 빈 패턴

이는 “비트 소비 없이 문법 조각 하나를 제공한다”는 의미다. 종종 optional operand의 기본값이나, 패턴 길이 변화 없는 문법 노드 구현에 사용된다.

#### 7.4.7 고급 제약

단순 `field = constant` 외에도 일반 표현식을 비교 조건으로 쓸 수 있다.

예시 개념:

* `r1 = r2` 인 경우를 별도 pseudo-instruction으로 분기
* 특정 조합일 때만 축약 표기 선택

주의:

* 이런 제약은 내부 파싱 상태 수를 크게 늘릴 수 있다
* 잘못 쓰면 겹치는 constructor를 증가시켜 유지보수를 어렵게 만든다

따라서 고급 제약은 “정말 별도 구문으로 보여야 하는가”를 먼저 검토하고 쓰는 것이 좋다.

---

### 7.5 Disassembly Actions Section

bit pattern 뒤의 `[...]` 구간이다.

목적:

* **실행 의미론**이 아니라 **디스어셈블 시점 계산**
* display용 동적 값 생성

대표 용도:

* 상대 분기 오프셋을 절대 주소로 환산
* context 변수 변경
* 표시 문자열에 들어갈 파생값 계산

핵심 특징:

* constructor 매칭 직후 실행된다
* 이후 오퍼랜드 해석에 영향을 줄 수 있다

#### pattern expression / general action

이 섹션에서는 필드, 상수, 컨텍스트, 산술식을 조합해 동적 값을 계산할 수 있다.

가장 흔한 실수는 이 섹션을 semantic section처럼 쓰려는 것이다. disassembly action은 어디까지나 **디코더가 다음 해석을 위해 참고할 값**을 만드는 곳이지, 프로그램 상태를 모델링하는 곳이 아니다.

예를 들어 분기 대상 주소를 계산해서 display에 넣는 것은 적절하지만, 일반 레지스터 변경을 여기서 처리하면 안 된다.

---

### 7.6 With Block

문서 목차에 존재하지만, 실무적으로는 constructor 문맥을 묶어 공통 속성/제약을 적용하는 개념으로 이해하면 된다.

* 반복되는 제약이나 문맥을 묶어 표현할 때 사용되는 구조
* 요지는 여러 constructor에 공통되는 해석 단위를 묶는 데 있다

큰 명세에서 같은 prefix 조건, 같은 context 조건, 같은 토큰 범위를 여러 constructor에 반복해서 붙이는 대신, with block으로 공통성을 추상화할 수 있다. 이는 코드량 감소뿐 아니라, 조건 불일치로 생기는 미묘한 디코딩 버그를 줄이는 데도 유리하다.

---

### 7.7 Semantic Section

중괄호 `{...}` 내부다.

여기서 실제 p-code 생성이 일어난다. 즉, 이 섹션이 Ghidra 분석기에게 “이 명령어는 결국 무엇을 하는가”를 알려준다.

좋은 semantic section의 기준은 다음과 같다.

* 레지스터/메모리 읽기와 쓰기가 명확하다
* 크기 정보가 모호하지 않다
* 플래그 변화가 누락되지 않는다
* 실제 ISA 의미와 최대한 직접적으로 대응된다

#### 7.7.1 표현식

##### 상수

* 정수 상수는 문맥에 따라 크기가 추론된다
* 필요하면 `0:4`처럼 크기를 명시한다

크기 추론이 애매할 때는 과감히 명시하는 편이 낫다. 특히 산술 연산, 비교, 메모리 주소 계산이 섞일 때 상수 폭을 명확히 쓰면 디버깅 시간이 줄어든다.

##### 메모리 dereference `*`

```sleigh
*:4 ptr
*[ram]:4 ptr
```

* 포인터를 따라 load
* 공간과 크기를 지정 가능

이는 매우 중요한 문법이다. 메모리 오퍼랜드가 등장하는 ISA에서 사실상 매일 쓰게 된다. 공간을 생략하면 default space 기준이 적용되므로, 다중 메모리 공간 ISA에서는 명시하는 편이 안전하다.

##### truncation

```sleigh
lo = r1:4;
hi = r1(4);
```

* 하위 바이트 추출 또는 하위 바이트 제거

`:`와 `()`의 의미를 혼동하기 쉽다. 하나는 하위 부분을 취하고, 다른 하나는 하위 부분을 버린다. 부분 레지스터 모델링이나 넓은 곱셈 결과 분리에서 자주 등장한다.

##### bit range 추출

```sleigh
x = r2[3,1];
```

* `(최하위 비트 위치, 비트 수)`
* 실제 p-code로는 shift / and / subpiece 조합으로 풀린다

##### address-of `&`

```sleigh
tmp:4 = &r1 + 4;
```

* 심볼의 **정적 주소 오프셋**을 얻는다
* 실행 시점 계산이 아니라 디스어셈블 시점 값이라는 점이 중요
* 동적 주소에는 부적합할 수 있다

이 부분은 자주 오해된다. `&`는 C 언어의 일반적인 런타임 주소 취득 연산과 동일하지 않다. SLEIGH에서는 “해당 심볼이 매핑된 정적 위치”를 표현하는 도구에 가깝다.

##### managed code 연산

* `cpool(...)`: constant pool 질의
* `newobject(...)`: 객체 할당

이는 주로 바이트코드/관리형 런타임 계열에서 중요하다. 일반 네이티브 ISA에서는 드물지만, JVM/DEX/.NET 계열 분석에는 핵심이 될 수 있다.

##### user-defined pcodeop 호출

```sleigh
r1 = arctan(r2);
```

분석 품질은 떨어지지만, 의미론을 완전히 비워 두는 것보다는 데이터 흐름을 남길 수 있다는 장점이 있다.

---

#### 7.7.2 문장(statement)

##### 대입과 임시 변수

```sleigh
local tmp:4 = r1 + r2;
```

* `local` 로 `unique` 공간 임시 변수 생성
* 이후 같은 semantic section에서 재사용 가능

복잡한 명령어는 임시 변수를 적절히 쓰는 편이 훨씬 읽기 쉽다. 특히 플래그 갱신과 결과 저장이 섞이는 경우, 한 줄로 모두 쓰기보다 단계적으로 풀어 쓰는 편이 유지보수에 유리하다.

##### export

`export`는 constructor가 최종적으로 대표하는 varnode를 밖으로 내보낸다.

* table/operand의 semantic 의미를 결정하는 핵심
* 동적 branch destination 표현에도 중요

하위 table이 단순히 display용인지, 실제 semantic operand를 제공하는지 구분되는 지점이 바로 `export`다. addressing mode table에서 자주 쓰인다.

##### branching

직접 분기:

```sleigh
goto dest;
call dest;
if (cond) goto dest;
```

간접 분기:

```sleigh
goto [reg];
call [reg];
return [tmp];
```

분기 명세에서 중요한 것은 **직접/간접**을 정확히 나누는 것이다. 간접 분기를 직접 분기로 잘못 모델링하면 control-flow graph 자체가 틀어질 수 있다.

##### p-code 내부 상대 분기

semantic section 내부 label 사용 가능:

```sleigh
<loop>
...
if (cond) goto <loop>;
```

* 같은 constructor 내부 p-code 흐름 제어용

이 기능은 고수준 제어 구조를 흉내 내기보다는, 짧은 p-code 시퀀스 내부에서 조건적 동작을 나눌 때 쓰는 도구로 이해하는 편이 좋다.

##### skip-instruction branching

* `inst_next2` 사용
* 다음 명령어 하나를 건너뛰는 아키텍처에서 유용

예를 들어 특정 조건이 참이면 “다음 명령어를 실행하지 않는다” 같은 ISA 특성을 모델링할 때 필요하다.

##### bit range assignment

```sleigh
r1[3,1] = 1;
```

* 특정 비트 범위만 갱신
* 나머지 비트는 유지

상태 레지스터의 개별 플래그 갱신에 특히 유용하다. 다만 bitrange와 섞어서 쓰면 실제 확장되는 p-code가 길어질 수 있으므로 결과를 검토하는 것이 좋다.

#### 7.7.3 varnode 크기 해결

크기 추론이 애매한 대표 원인:

* 상수
* 임시 변수
* 메모리 dereference
* 부분 추출 결과

해결 방법:

* `*:4 ptr` 처럼 load/store 크기 명시
* `tmp:4` 처럼 temp 크기 명시
* `0:4` 처럼 상수 크기 명시

추론 실패 시 SLEIGH 컴파일러가 오류를 낸다.

실전 팁은 단순하다. **모호하면 써라.** 명세 작성 초기에 크기를 생략해서 코드를 짧게 만드는 것보다, 처음부터 폭을 명시해 디버깅을 줄이는 편이 대개 이득이다.

#### 7.7.4 unimplemented semantics

```sleigh
:cache r1 is opcode=0x45 & r1 unimpl
```

* 디스어셈블은 되지만 데이터 흐름 분석은 오류 처리될 수 있다
* 개발 중 placeholder 또는 분석 제외 명령어에 사용

이 기능은 임시로는 유용하지만, 장기적으로 많이 남아 있으면 분석 품질의 블라인드 스팟이 된다. 따라서 가능하면 TODO 목록과 연결해 관리하는 편이 좋다.

---

### 7.8 Tables

table은 하나 이상의 constructor를 묶어 새 family symbol을 만든다.

#### matching 규칙

한 table 내부 constructor들 패턴은 다음 관계여야 한다.

* 서로 완전히 disjoint
* 또는 한쪽이 다른 쪽의 특수화(special case)

문제 상황:

* 겹치지만 포함 관계가 아니면 일반적으로 명세 오류
* lenient 모드에서는 먼저 등장한 constructor가 선택될 수 있으나 권장되지 않음

이 규칙은 문법의 모호성을 줄이기 위한 핵심 제약이다. 대형 ISA 명세에서 자주 생기는 문제는, 처음에는 disjoint했던 규칙이 나중에 확장 opcode를 추가하면서 은근히 겹치기 시작한다는 점이다. 따라서 새 constructor를 추가할 때는 기존 특수화 관계를 반드시 재검토해야 한다.

#### specific symbol tree

파싱은 `instruction` 루트에서 시작해 하위 operand/table로 재귀적으로 내려간다. 결과적으로 명령어 하나는 **specific symbol tree** 로 표현된다.

이 트리에서:

* display 정보를 모으면 디스어셈블리 문장이 된다
* p-code를 모으면 최종 semantic translation이 된다

즉, SLEIGH는 트리 기반으로

1. 명령어를 분해하고
2. 디스어셈블 문자열을 조립하고
3. p-code를 depth-first 순서로 합성한다

이 개념을 이해하면 왜 하위 operand constructor에도 semantic section과 export가 있을 수 있는지 자연스럽게 보인다. operand는 단순 문자열 조각이 아니라, 독립적인 의미론 단위일 수 있다.

---

### 7.9 p-code Macro

```sleigh
macro resultflags(op) {
  zeroflag = (op == 0);
  signflag = (op s< 0);
}
```

특징:

* semantic action 재사용용
* 컴파일 시 확장된다
* 파라미터는 참조로 전달된다
* 반환값 전용 문법은 없고, 파라미터/전역 심볼 갱신으로 효과를 낸다
* 대부분의 semantic statement를 포함 가능
* `build`는 macro 안에서 쓰지 않는 것이 권장된다

플래그 갱신 규칙이 반복되는 ISA에서는 거의 필수 수준이다. add/sub/logic/shift 계열에서 공통 플래그 설정이 있다면 macro로 빼 두는 편이 훨씬 낫다.

다만 너무 큰 macro는 오히려 추적이 어렵다. 보통은 “플래그 세트 하나”, “주소 계산 한 조각” 정도 크기가 적당하다.

---

### 7.10 build directive

```sleigh
build cc;
```

기본적으로 child operand p-code는 depth-first 순서로 생성되며, 디자이너가 순서를 직접 제어하기 어렵다. `build`는 특정 operand의 p-code를 **바로 이 지점에서** 생성하게 강제한다.

유용한 상황:

* prefix 명령어
* addressing mode side effect
* 조건부 실행 접두사
* 특정 오퍼랜드 평가 시점이 중요할 때

이 지시문은 “평가 순서”를 조정하는 도구로 이해하면 된다. 평상시에는 필요 없지만, side effect가 있는 operand나 mode-dependent 해석에서는 매우 중요해질 수 있다.

---

### 7.11 delayslot directive

```sleigh
delayslot(1);
```

* branch delay slot을 가진 아키텍처 지원용
* 지정한 바이트 수 이상이 될 때까지 다음 명령어(들)를 파싱해서 해당 위치에 p-code 삽입
* 보통 `1`은 다음 명령어 하나를 의미

delay slot이 있는 ISA를 다룰 때는 control-flow와 semantics가 일반 CPU와 다르기 때문에, 이 지시문을 빼먹으면 디컴파일이 매우 어색해질 수 있다. 특히 분기 직후 한 명령이 항상 실행되는 구조를 정확히 반영해야 한다.

---

## 8. Context 사용법

context는 **명령어 비트만으로는 부족한 디코딩 정보**를 다룬다.

대표 상황:

1. 하나의 CPU가 여러 instruction set / mode를 지원
2. 어떤 명령어가 다음 명령어의 해석 방식에 영향을 줌
3. prefix나 상태 비트에 따라 레지스터 뱅크가 바뀜
4. 분기 후 특정 주소 범위에서 해석 규칙이 달라짐

### 8.1 기본 사용

context variable을 일반 필드처럼 패턴에서 사용한다.

예:

* `mode=0` 이면 일반 레지스터군 사용
* `mode=1` 이면 다른 레지스터군 사용

즉, 같은 인코딩도 context 상태에 따라 다른 어셈블리로 보일 수 있다.

이 기능은 코드 밀도 높은 아키텍처에서 매우 중요하다. 동일 바이트열이 현재 모드에 따라 완전히 다른 명령어가 되는 경우가 실제로 존재한다.

### 8.2 local context change

disassembly action에서 context를 바꾸면 **현재 명령어 해석 중**에만 영향을 줄 수 있다.

```sleigh
[ mode=1; ]
```

용도:

* 특정 opcode가 뒤쪽 오퍼랜드 해석 규칙을 바꾸는 경우
* 현재 constructor 내부에서만 모드 강제
* 하나의 명령어 안에 이질적 서브-포맷이 섞이는 경우

특징:

* 조상 constructor 매칭에는 영향 없음
* 이후 다른 명령어에는 자동 지속되지 않음

즉, local context는 “현재 parse subtree 내부의 렌즈”처럼 생각할 수 있다.

### 8.3 global context change

```sleigh
[ mode=1; globalset(inst_next,mode); ]
```

* 전역 context 상태 변경을 디스어셈블 엔진에 제안
* 보통 다음 주소부터 모드가 바뀌도록 `inst_next` 사용

#### context flow

기본적으로 global context 변화는 **제어 흐름을 따라 전파**된다.

* 분기/호출을 따라 영향을 줄 수 있다
* 다른 context 변화가 나올 때까지 이어질 수 있다

`noflow`를 붙이면:

* 지정 주소의 **한 개 명령어**에만 적용
* 다음 명령어들에는 이어지지 않는다

이 패턴은 “바로 직전 명령이 다음 명령의 의미를 바꾸는 경우”에 유용하다.

주의:

* 실제 어떤 주소에서 어떤 global context가 유효한지는 디스어셈블 과정 전체에 의존한다
* 따라서 충돌/불확실성은 SLEIGH 자체보다 디스어셈블 엔진 쪽 문제다

### context 설계 팁

* 진짜로 외부 상태가 필요한 경우에만 사용하라.
* 단순 인코딩 차이는 field/attach/table로 먼저 해결하라.
* global context는 강력하지만 디버깅 비용이 크므로 최소화하라.
* prefix 하나가 다음 한 명령에만 영향 주는 구조라면 `noflow` 패턴을 우선 검토하라.

즉, context는 구명줄이지만 기본 도구는 아니다. 남용하면 명세가 눈에 보이지 않는 상태에 의존하게 된다.

---

## 9. p-code 연산/문장 레퍼런스 요약

아래는 문서의 p-code 표를 실무 참고용으로 다시 분류한 것이다. 원문 표는 연산명을 기준으로 나열되어 있지만, 실제 작성할 때는 “무슨 종류의 작업을 하려는가” 기준으로 묶어 보는 편이 더 빠르다.

### 9.1 표현식 연산자

#### 절단/비트 조작

* `v0:2` : 하위 n바이트 취득
* `v0(2)` : 하위 n바이트 제거
* `v0[6,1]` : 특정 비트 범위 추출
* `popcount(v0)` : 1비트 개수
* `lzcount(v0)` : leading zero 개수

이 부류는 부분 레지스터 접근, flag 생성, 비트 필드 추출, 넓은 결과 분할에 자주 쓰인다.

#### 메모리 로드

* `*v1`
* `*[spc]v1`
* `*:2 v1`
* `*[spc]:2 v1`

메모리 관련 버그는 대개 공간 또는 폭 지정 누락에서 생긴다. default space에 지나치게 의존하지 않는 것이 안전하다.

#### 단항 연산

* `!v0` : boolean negate
* `~v0` : bitwise negate
* `-v0` : 2의 보수
* `f- v0` : float negate

특히 `!`와 `~`를 혼동하지 않아야 한다. 전자는 불리언, 후자는 비트 반전이다.

#### 산술

* `v0 * v1`
* `v0 / v1`
* `v0 s/ v1`
* `v0 % v1`
* `v0 s% v1`
* `v0 + v1`
* `v0 - v1`
* `v0 f+ v1`
* `v0 f- v1`
* `v0 f* v1`
* `v0 f/ v1`

정수/부동소수, signed/unsigned를 분명히 나눠 써야 한다. 이 구분이 틀리면 비교 결과, 오버플로 플래그, 디컴파일 타입 추론이 연쇄적으로 흔들릴 수 있다.

#### 시프트

* `v0 << v1`
* `v0 >> v1`
* `v0 s>> v1`

논리 시프트와 산술 시프트를 반드시 구분해야 한다. 부호 비트 보존 여부가 달라지기 때문이다.

#### 정수 비교

* `v0 s< v1`, `v0 s<= v1`
* `v0 < v1`, `v0 <= v1`
* `v0 == v1`, `v0 != v1`

#### 부동소수 비교

* `v0 f< v1`, `v0 f<= v1`
* `v0 f== v1`, `v0 f!= v1`

조건 분기 명세에서 signed/unsigned 비교를 틀리는 것은 가장 치명적인 실수 중 하나다.

#### 비트/불리언 논리

* `v0 & v1`
* `v0 ^ v1`
* `v0 | v1`
* `v0 ^^ v1`
* `v0 && v1`
* `v0 || v1`

비트 단위 논리와 불리언 논리가 모두 존재하므로, 플래그 조합을 쓸 때 의도를 명확히 해야 한다.

#### 확장/오버플로/변환

* `zext(v0)`
* `sext(v0)`
* `carry(v0,v1)`
* `scarry(v0,v1)`
* `sborrow(v0,v1)`
* `nan(v0)`
* `abs(v0)`
* `sqrt(v0)`
* `int2float(v0)`
* `float2float(v0)`
* `trunc(v0)`
* `ceil(v0)`
* `floor(v0)`
* `round(v0)`

이 그룹은 플래그 모델링과 타입 경계 변환에서 중요하다. 특히 carry/scarry/sborrow는 condition code 계산의 핵심이다.

#### 특수 연산

* `cpool(v0,...)`
* `newobject(v0)`
* `ident(v0,...)` : user-defined op

### 9.2 기본 statement

* `v0 = v1;`
* `*v0 = v1;`
* `*[spc]v0 = v1;`
* `*:4 v0 = v1;`
* `ident(v0,...);` : user-defined op statement
* `v0[8,1] = v1;` : bit range assignment
* `ident(v0,...);` : macro 호출
* `build ident;`
* `delayslot(1);`

statement는 결국 상태 변화를 만드는 단위다. 표현식이 값을 만들고, statement가 그 값을 저장하거나 제어 흐름에 반영한다.

### 9.3 분기 statement

* `goto v0;`
* `if (v0) goto v1;`
* `goto [v0];`
* `call v0;`
* `call [v0];`
* `return [v0];`

분기 계열에서는 “주소가 상수인지, 계산값인지, 메모리/레지스터를 통해 간접인지”를 정확히 구분하는 것이 핵심이다.

---

## 10. SLEIGH 작성 실전 흐름

문서 전체를 실무 흐름으로 압축하면 대체로 다음 순서다.

1. 엔디언/정렬/주소 공간 정의
2. 레지스터 및 비트레인지 정의
3. 토큰과 필드 정의
4. `attach`로 필드 의미 연결
5. context가 필요하면 context 변수 정의
6. table + constructor로 오퍼랜드와 명령어 계층 구성
7. display section에서 어셈블리 모양 정의
8. bit pattern에서 인코딩 제약 정의
9. disassembly action에서 상대 주소/동적 값 계산
10. semantic section에서 p-code 기술
11. 공통 로직은 macro로 추출
12. 필요 시 build / delayslot / globalset 사용

### 실무식으로 다시 풀어 쓴 절차

#### 10.1 먼저 레지스터와 메모리 모델을 확정한다

명령어를 쓰기 전에 레지스터 폭, alias 구조, 상태 레지스터, 기본 메모리 공간부터 고정해야 한다. 이 단계가 흔들리면 뒤의 semantic이 전부 흔들린다.

#### 10.2 인코딩 표를 토큰/필드로 옮긴다

ISA 매뉴얼의 bit layout을 가능한 한 직접 대응되게 토큰화하되, 지나치게 CPU 문서 형식에 묶이지는 않는다. SLEIGH에서 관리하기 편한 구조가 더 중요하다.

#### 10.3 오퍼랜드를 먼저 table로 분리한다

복잡한 명령어를 바로 쓰기보다, 레지스터 오퍼랜드, 즉시값, 메모리 오퍼랜드, 분기 대상 계산을 개별 table로 분리한다. 이렇게 해야 재사용성과 디버깅성이 좋아진다.

#### 10.4 명령어 constructor를 점진적으로 추가한다

처음부터 모든 변형을 덮으려 하지 말고, 핵심 명령군부터 추가한다. 예를 들면:

* move/load/store
* arithmetic
* compare/branch
* call/return
* shift/bit ops

이 순서가 좋다. 제어 흐름과 데이터 이동이 먼저 안정돼야 분석 결과가 빨리 보인다.

#### 10.5 p-code를 실제 CPU 매뉴얼과 대조한다

semantic section은 “어셈블리 문장이 그럴듯한가”가 아니라, **실제 상태 변화가 맞는가**가 기준이다. 결과 레지스터, 메모리, flags, PC 갱신 규칙을 매뉴얼과 줄 단위로 대조하는 습관이 중요하다.

#### 10.6 Ghidra 결과를 보고 다시 역검증한다

좋은 SLEIGH 작성은 소스만 보고 끝나지 않는다. 실제 바이너리를 넣고 다음을 확인해야 한다.

* 디스어셈블리가 자연스러운가
* 잘못 분기되는 패턴은 없는가
* p-code가 예상대로 생성되는가
* 디컴파일이 이상한 타입/조건식으로 흐르지 않는가

---

## 11. 자주 헷갈리는 핵심 포인트

### 11.1 field의 의미와 pattern의 의미는 다를 수 있다

`attach values`나 `attach variables`로 필드 의미를 바꿔도, **pattern constraint에서는 원래 비트 인코딩 값**을 기준으로 비교한다.

즉, “출력/semantic에서의 의미”와 “패턴 매칭에서의 원래 숫자값”을 구분해야 한다.

### 11.2 register_space는 포인터 aliasing이 없다고 가정한다

`register_space`로 둔 공간은 간접 포인터 접근 대상이 아니라고 분석기가 본다. 실제 아키텍처 성질과 맞아야 한다.

레지스터 파일을 그냥 메모리처럼 다루는 구조를 가진 ISA라면, 이 가정이 맞는지 신중히 검토해야 한다.

### 11.3 bitrange는 보기보다 무겁다

1비트 플래그를 레지스터처럼 정의할 수 있지만, 실제 p-code는 여러 연산으로 확장될 수 있다.

따라서 플래그가 매우 많은 아키텍처에서는 가독성과 성능 사이 균형을 고려해야 한다.

### 11.4 address-of는 실행 중 계산이 아니다

`&symbol`은 disassembly 시점 정적 주소 계산이다. 런타임 포인터 연산과 혼동하면 안 된다.

### 11.5 size ambiguity는 명시적으로 풀어야 한다

* `*:4`
* `tmp:4`
* `0:4`

같은 방식으로 크기를 직접 써 주는 것이 가장 빠른 해결책이다.

### 11.6 overlapping constructor는 신중해야 한다

한 table 안의 constructor는 가급적 disjoint하거나 명확한 specialization 관계여야 한다.

애매한 겹침은 “어느 패턴이 잡히는지”를 사람도 컴파일러도 이해하기 어렵게 만든다.

### 11.7 context는 강력하지만 엔진 의존성이 있다

특히 `globalset`은 “문맥을 영구 변경한다”기보다 **디스어셈블 엔진에 그 변화를 알려준다**는 성격이 강하다.

따라서 코드 배치, 진입점, 제어 흐름 발견 상태에 따라 실제 해석 영향이 달라질 수 있다.

### 11.8 디스플레이가 맞아도 semantics가 틀릴 수 있다

SLEIGH 초반 작업에서 가장 흔한 함정이다. 디스어셈블 문자열이 멀쩡하면 안심하기 쉬운데, 실제로는 p-code가 틀린 경우가 많다.

예:

* signed/unsigned 비교 반전
* carry/borrow 플래그 계산 누락
* write-back 주소 지정 누락
* delay slot 반영 누락
* prefix 효과를 display에만 반영하고 semantics에 반영하지 않음

### 11.9 pseudo-instruction은 보기 좋지만 분석을 흐릴 수도 있다

특정 패턴을 더 예쁜 mnemonic으로 축약하는 것은 유용하지만, 너무 많이 하면 실제 ISA 의미와 멀어질 수 있다. 특히 분석/검색 기준이 실제 opcode군과 어긋날 수 있으므로 신중히 사용해야 한다.

---

## 12. 최소 예시 템플릿

```sleigh
define endian=little;
define alignment=1;

define space ram type=ram_space size=4 default;
define space register type=register_space size=4;

define register offset=0 size=4 [ r0 r1 r2 r3 ];

define token instr(16)
  opcode=(12,15)
  dst=(8,11)
  src=(4,7)
  imm=(0,7)
;

attach variables [ dst src ] [ r0 r1 r2 r3 ];

:add dst,src is opcode=0x1 & dst & src {
  dst = dst + src;
}

:mov dst,#imm is opcode=0x2 & dst & imm {
  dst = imm;
}
```

이 템플릿만으로도 다음 요소를 모두 담는다.

* 기본 공간/레지스터 정의
* 토큰/필드 정의
* attach variables
* constructor display/pattern/semantic section

### 이 템플릿을 읽는 법

* `opcode`는 명령어 종류를 고르는 필드
* `dst`, `src`는 비트값이지만 attach를 통해 레지스터로 해석됨
* 첫 constructor는 레지스터-레지스터 덧셈
* 두 번째 constructor는 즉시값 이동

즉, 매우 작은 예제지만 SLEIGH의 핵심 흐름 전체가 들어 있다. 실제 ISA 명세는 이 패턴이 수백~수천 번 반복되고, 중간에 하위 table과 context와 action이 끼어드는 형태라고 보면 된다.

### 확장 예시 아이디어

이 최소 템플릿을 다음 순서로 확장하면, 작은 학습용 ISA를 금방 만들 수 있다.

1. `sub`, `and`, `or` 추가
2. 메모리 load/store 추가
3. relative branch 추가
4. zero/sign flag 추가
5. conditional branch 추가
6. call/return 추가

이런 식으로 점진적으로 늘려 가면 SLEIGH 학습이 빠르다.

---

## 13. 문서 전체를 한 문장으로 묶으면

SLEIGH는 **명령어 인코딩, 디스어셈블리 표시, 의미론적 p-code 번역**을 하나의 규칙 체계로 묶는 언어다. 실무적으로는 다음 4가지를 제대로 설계하는 것이 거의 전부라고 봐도 된다.

1. **비트가 무엇을 의미하는가** (`token`, `field`, `attach`)
2. **그 의미가 어떤 문법 단위로 조립되는가** (`constructor`, `table`)
3. **디스어셈블 문자열이 어떻게 보이는가** (`display`, `disassembly actions`)
4. **최종 p-code가 어떻게 생성되는가** (`semantic section`, `macro`, `build`, `delayslot`, `context`)

이 네 축이 맞물리면 Ghidra는 해당 ISA를 디스어셈블하고, p-code로 올리고, 그 위에서 디컴파일과 데이터 흐름 분석을 수행할 수 있다.

반대로 말하면, SLEIGH 작업의 난점도 이 네 축이 서로 연결돼 있다는 데 있다. 한 축에서의 작은 모델링 실수가 다른 축에서 크게 증폭될 수 있다. 예를 들어 필드 해석을 잘못 붙이면:

* 잘못된 레지스터가 출력되고
* 잘못된 semantic operand가 선택되고
* 잘못된 p-code가 생성되고
* 결국 디컴파일 전체가 이상해진다

따라서 SLEIGH 명세 작성은 “문법 파일 작성”이 아니라, 사실상 **프로세서 의미 모델 구현**에 가깝다.

---

## 14. 마지막 정리: 실제로 무엇을 익혀야 하나

이 문서를 다 읽고 나면, 실무 기준으로는 아래 순서대로 익히는 것이 가장 효율적이다.

### 14.1 가장 먼저 익힐 것

* `define space`, `define register`
* `define token`, field 범위 지정
* `attach variables`, `attach values`, `attach names`
* 기본 constructor 문법

### 14.2 그다음 익힐 것

* memory operand를 export하는 하위 table 작성
* relative branch 대상 계산
* 플래그 갱신용 p-code macro
* bitrange와 부분 레지스터 처리

### 14.3 나중에 익혀도 되는 것

* global context flow
* build directive
* delayslot
* user-defined pcodeop
* managed code 관련 연산

### 14.4 학습 순서 제안

가장 좋은 학습법은 완성된 대형 ISA를 바로 읽는 것이 아니라, 작은 장난감 ISA를 직접 만들어 보는 것이다. 그 과정에서 다음 질문에 스스로 답할 수 있으면 SLEIGH 핵심을 이해한 것이다.

* 이 비트 필드는 어디서 정의되는가
* 이 오퍼랜드는 왜 table로 분리되었는가
* 이 display string은 어떻게 조립되는가
* 이 semantic section은 왜 이 순서로 p-code를 내는가
* 이 context 변화는 왜 local이 아니라 global인가

이 질문에 답할 수 있으면, Ghidra 공식 문서의 세부 문법도 훨씬 빠르게 소화된다.

---

## 15. 한 줄 결론

SLEIGH는 **기계어를 읽는 규칙**이 아니라, **CPU 명령어 체계를 Ghidra가 이해할 수 있는 형태로 완전하게 기술하는 언어**다. 즉, 잘 만든 SLEIGH 명세는 단순한 디스어셈블러 정의가 아니라, 디컴파일과 정적 분석까지 떠받치는 ISA 의미 모델이다.
