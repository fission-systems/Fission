typedef unsigned int u32;
typedef unsigned char u8;

volatile u32 control_sink = 0;

u32 test_switch(u32 val) {
    u32 res = 0;
    switch (val % 5) {
        case 0:
            res = val * 2;
            break;
        case 1:
            res = val + 10;
            break;
        case 2:
            res = val ^ 0xAAAA;
            break;
        case 3:
            res = val - 5;
            break;
        case 4:
        default:
            res = val;
            break;
    }
    return res;
}

u32 test_loops(u32 count) {
    u32 acc = 0;
    for (u32 i = 0; i < count; ++i) {
        if (i % 3 == 0) continue;
        if (acc > 0x10000000) break;
        acc += test_switch(i);
    }
    
    u32 j = count;
    while (j > 0) {
        acc ^= j;
        j >>= 1;
    }
    
    do {
        acc++;
        count--;
    } while (count > 0 && acc % 2 != 0);
    
    return acc;
}

u32 test_nested_loops(u32 rows, u32 cols) {
    u32 sum = 0;
    for (u32 i = 0; i < rows; ++i) {
        if (i % 2 == 0) continue;
        for (u32 j = 0; j < cols; ++j) {
            if (j == 5) break;
            sum += i * j;
        }
        if (sum > 500) break;
    }
    return sum;
}

u32 test_switch_fallthrough(u32 val) {
    u32 res = 0;
    switch (val) {
        case 1:
        case 2:
            res += 5;
            // fallthrough
        case 3:
            res += 10;
            break;
        case 4:
            res = 100;
            break;
        default:
            res = val * 3;
            break;
    }
    return res;
}

void run_control_flow(u32 seed) {
    u32 result = 0;
    if (seed < 10) {
        result = test_switch(seed) + test_switch_fallthrough(seed);
    } else if (seed < 100) {
        result = test_loops(seed) + test_nested_loops(seed, 10);
    } else {
        result = test_loops(100) + test_switch(seed) + test_nested_loops(10, seed % 20) + test_switch_fallthrough(seed);
    }
    control_sink = result;
}

