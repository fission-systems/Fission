fn main() {
    println!("Running Incremental SSA Partitioning tests...");

    // Test 1: disjoint partitions
    let accesses = vec![(0, 2), (2, 2)];
    let res = fission_pcode::midend::test_refine_partitions(&accesses);
    assert_eq!(res, vec![(0, 2), (2, 2)]);
    println!("test_disjoint_partitions passed!");

    // Test 2: spanned partitions
    let accesses = vec![(0, 4), (0, 2), (2, 2)];
    let res = fission_pcode::midend::test_refine_partitions(&accesses);
    assert_eq!(res, vec![(0, 4)]);
    println!("test_spanned_partitions passed!");

    // Test 3: 1/3-byte merge aligned
    let accesses = vec![(0, 1), (1, 3)];
    let res = fission_pcode::midend::test_refine_partitions(&accesses);
    assert_eq!(res, vec![(0, 4)]);

    let accesses2 = vec![(0, 3), (3, 1)];
    let res2 = fission_pcode::midend::test_refine_partitions(&accesses2);
    assert_eq!(res2, vec![(0, 4)]);
    println!("test_1_3_merge_aligned passed!");

    // Test 4: 1/3-byte merge unaligned
    let accesses = vec![(2, 1), (3, 3)];
    let res = fission_pcode::midend::test_refine_partitions(&accesses);
    assert_eq!(res, vec![(2, 1), (3, 3)]);
    println!("test_1_3_merge_unaligned passed!");

    println!("All Incremental SSA Partitioning tests passed successfully!");
}
