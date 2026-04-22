fn bit_reverse(mut value: u32) -> u32 {
    let mut result: u32 = 0;
    for _ in 0..32 {
        result = (result << 1) | (value & 1);
        value >>= 1;
    }
    result
}

fn popcount(mut value: u32) -> i32 {
    let mut count = 0;
    while value > 0 {
        count += (value & 1) as i32;
        value >>= 1;
    }
    count
}

fn find_first_set_bit(mut value: u32) -> i32 {
    if value == 0 {
        return -1;
    }

    let mut pos = 0;
    while (value & 1) == 0 {
        value >>= 1;
        pos += 1;
    }
    pos
}

fn xor_all(values: &[u32]) -> u32 {
    values.iter().fold(0, |acc, &v| acc ^ v)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    Active,
    Processing,
    Error,
}

struct StateMachine {
    current: State,
}

impl StateMachine {
    fn new() -> Self {
        StateMachine {
            current: State::Idle,
        }
    }

    fn transition(&mut self, input: i32) -> State {
        self.current = match self.current {
            State::Idle => {
                if input == 1 {
                    State::Active
                } else {
                    State::Idle
                }
            }
            State::Active => {
                if input == 0 {
                    State::Idle
                } else if input == 2 {
                    State::Processing
                } else {
                    State::Active
                }
            }
            State::Processing => {
                if input == 3 {
                    State::Error
                } else if input == 0 {
                    State::Idle
                } else {
                    State::Processing
                }
            }
            State::Error => {
                if input == 1 {
                    State::Idle
                } else {
                    State::Error
                }
            }
        };
        self.current
    }

    fn get_state(&self) -> State {
        self.current
    }
}

fn validate_input(x: i32, y: i32) -> i32 {
    match (x, y) {
        _ if x < 0 => -1,
        _ if y < 0 => -2,
        _ if x > 1000 || y > 1000 => -3,
        _ => x + y,
    }
}

fn complex_switch(a: i32, b: i32) -> i32 {
    match a {
        1 => match b {
            10 => 100,
            20 => 200,
            _ => 0,
        },
        2 => match b {
            10 => 1000,
            20 => 2000,
            _ => 0,
        },
        _ => -1,
    }
}

fn complex_loop(arr: &[i32]) -> i32 {
    let mut result = 0;

    for &val in arr {
        if val < 0 {
            continue;
        }
        if val > 1000 {
            break;
        }

        for _ in 0..val {
            result += 1;
            if result > 10000 {
                break;
            }
        }
    }

    result
}

fn process_matrix(matrix: &[Vec<i32>]) -> i32 {
    matrix
        .iter()
        .flat_map(|row| row.iter())
        .filter(|&&x| x > 0)
        .sum()
}

fn main() {
    // Test bit operations
    let bf_value: u32 = 0xDEADBEEF;
    let reversed = bit_reverse(bf_value);
    let popcount_result = popcount(bf_value);
    let first_set = find_first_set_bit(bf_value);

    // Test state machine
    let mut fsm = StateMachine::new();
    let s1 = fsm.transition(1);
    let s2 = fsm.transition(2);
    let s3 = fsm.transition(3);
    let s4 = fsm.get_state();

    // Test bit operations
    let vals: [u32; 3] = [0x12345678, 0x87654321, 0xFFFFFFFF];
    let xor_result = xor_all(&vals);

    // Test control flow
    let validate_result = validate_input(500, 600);
    let switch_result = complex_switch(2, 20);
    let arr = vec![100, 200, 50, 75];
    let loop_result = complex_loop(&arr);

    // Test matrix operations
    let matrix = vec![
        vec![1, 2, 3],
        vec![4, 5, 6],
        vec![7, 8, 9],
    ];
    let matrix_sum = process_matrix(&matrix);

    println!(
        "Bitops and control flow test: {} {} {}",
        popcount_result, switch_result, matrix_sum
    );
    println!(
        "Bit reverse: {:x}, XOR: {:x}, Validate: {}",
        reversed, xor_result, validate_result
    );
    println!("Loop result: {}, First set bit: {}", loop_result, first_set);
}
