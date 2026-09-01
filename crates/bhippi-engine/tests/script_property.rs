#![allow(clippy::expect_used, clippy::panic)]

use bhippi_engine::script::{compile, OpCode, ScriptProgram};

fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    *state
}

fn assert_program_bounds(program: &ScriptProgram) {
    assert!(program.step_budget > 0);
    assert!(program.call_depth > 0);
    assert!(program.numbers.iter().all(|value| value.is_finite()));
    for function in &program.functions {
        assert!((function.entry as usize) < program.code.len());
        assert!(function.params <= function.locals);
    }
    for hook in &program.hooks {
        assert!((hook.function as usize) < program.functions.len());
    }
    for instruction in &program.code {
        let a = usize::try_from(instruction.a).ok();
        match instruction.op {
            OpCode::PushNum => assert!(a.is_some_and(|index| index < program.numbers.len())),
            OpCode::PushStr => assert!(a.is_some_and(|index| index < program.strings.len())),
            OpCode::PushBool => assert!(matches!(instruction.a, 0 | 1)),
            OpCode::Load | OpCode::Store => assert!(instruction.a >= 0),
            OpCode::Jump
            | OpCode::JumpIfFalse
            | OpCode::JumpIfFalsePeek
            | OpCode::JumpIfTruePeek => {
                assert!(a.is_some_and(|index| index < program.code.len()));
            }
            OpCode::CallHost => {
                assert!(a.is_some_and(|index| index < program.hosts.len()));
                assert!(instruction.b >= 0);
            }
            OpCode::CallUser => {
                assert!(a.is_some_and(|index| index < program.functions.len()));
                assert!(instruction.b >= 0);
            }
            OpCode::PushUnit
            | OpCode::Pop
            | OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Rem
            | OpCode::Neg
            | OpCode::Not
            | OpCode::Eq
            | OpCode::Ne
            | OpCode::Lt
            | OpCode::Le
            | OpCode::Gt
            | OpCode::Ge
            | OpCode::Return => {}
        }
    }
    serde_json::to_vec(program).expect("valid compiler output is serializable");
}

#[test]
fn generated_sources_are_located_rejection_or_bounded_valid_bytecode() {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_{}()[];,+-*/=!<>\". \n";
    let mut state = 0xC011_1E12_F022_0042_u64;
    for case_index in 0..2_048_usize {
        let length = (next(&mut state) as usize) % 257;
        let mut source = String::with_capacity(length);
        for _ in 0..length {
            source.push(ALPHABET[(next(&mut state) as usize) % ALPHABET.len()] as char);
        }
        match compile(&format!("generated-{case_index}.rhai"), &source) {
            Ok(program) => assert_program_bounds(&program),
            Err(fault) => {
                assert!(fault.line > 0 || source.is_empty());
                assert!(!fault.message.trim().is_empty());
            }
        }
    }
}
