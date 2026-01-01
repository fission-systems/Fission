use fission::core::prelude::*;
use proptest::prelude::*;

// -----------------------------------------------------------------------------
// Snapshot Testing Example using 'insta'
// -----------------------------------------------------------------------------
// 실제 디컴파일 결과가 변경되었을 때 개발자가 리뷰하고 승인(accept)하는 워크플로우를 제공합니다.
// 실행: cargo test --test advanced_tests
// 결과 업데이트: cargo insta review

#[test]
fn test_decompilation_snapshot_example() {
    // 가상의 디컴파일 결과
    let decompiled_output = serde_json::json!({
        "function": "main",
        "address": "0x140001000",
        "code": "int main(int argc, char** argv) { ... }",
        "meta": {
            "stack_size": 24,
            "complexity": 5
        }
    });

    // 스냅샷과 비교 (최초 실행 시 'snapshots/advanced_tests__decompilation_snapshot_example.snap' 생성)
    insta::assert_yaml_snapshot!(decompiled_output);
}

// -----------------------------------------------------------------------------
// Property-based Testing Example using 'proptest'
// -----------------------------------------------------------------------------
// 무작위 입력값에 대해 로직이 견고한지(패닉하지 않는지) 검증합니다.

// 테스트할 가상의 파서 함수
fn parse_header(data: &[u8]) -> Result<u32, &'static str> {
    if data.len() < 4 {
        return Err("Too short");
    }
    // 의도적인 엣지 케이스: 첫 바이트가 0xFF이면 패닉? (테스트로 잡아야 함)
    // if data[0] == 0xFF { panic!("Bug triggered!"); }

    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok(magic)
}

proptest! {
    #[test]
    fn test_parser_does_not_crash(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
        // 어떤 byte array가 들어와도 절대 패닉하면 안 됨 (Result 반환은 OK)
        let _ = parse_header(&data);
    }

    #[test]
    fn test_parser_valid_magic(magic in any::<u32>()) {
        let bytes = magic.to_le_bytes();
        let parsed = parse_header(&bytes).unwrap();
        prop_assert_eq!(parsed, magic);
    }
}
