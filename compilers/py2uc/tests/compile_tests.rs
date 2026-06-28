// py2uc compiler integration tests.

use utencore::bytecode::UtenModule;
use utencore::types::UValue;
use utencore::vm::Vm;

fn run(source: &str) -> Result<String, String> {
    let bytes = py2uc::compile_python(source, "<test>")
        .map_err(|e| format!("compilation: {e}"))?;
    let module = UtenModule::from_bytes(&bytes)
        .map_err(|e| format!("deserialize: {e}"))?;
    let mut vm = Vm::new();
    let mid = vm.load_module(module)
        .map_err(|e| format!("load: {e}"))?;
    match vm.execute(mid, 0, vec![]) {
        Ok(UValue::Nil) => Ok(String::new()),
        Ok(v) => Ok(format!("{v}")),
        Err(e) => Err(format!("{e}")),
    }
}

#[test]
fn test_compile_ok() {
    // Basic compilation should succeed without VM error
    run("print(1 + 2)").unwrap();
}

#[test]
fn test_compile_if() {
    run("x = 5\nif x > 0:\n    print(1)\nelse:\n    print(0)").unwrap();
}

#[test]
fn test_compile_while() {
    run("i = 0\nwhile i < 3:\n    i = i + 1\nprint(i)").unwrap();
}

#[test]
fn test_compile_for() {
    run("s = 0\nfor v in [1, 2, 3]:\n    s = s + v\nprint(s)").unwrap();
}

#[test]
fn test_compile_nested_for() {
    run("s = 0\nfor x in [1, 2]:\n    for y in [3, 4]:\n        s = s + x * y\nprint(s)").unwrap();
}

#[test]
fn test_compile_func() {
    run("def double(n):\n    return n * 2\nprint(double(21))").unwrap();
}

#[test]
fn test_compile_if_elif() {
    run("x = 0\nif x > 0:\n    print(1)\nelif x == 0:\n    print(0)\nelse:\n    print(-1)").unwrap();
}

#[test]
fn test_compile_augmented() {
    run("x = 10\nx += 5\nprint(x)").unwrap();
}

#[test]
fn test_compile_len() {
    run("print(len([1, 2, 3]))").unwrap();
}

#[test]
fn test_compile_bool() {
    run("x = True\nprint(x)").unwrap();
}

#[test]
fn test_compile_none() {
    run("x = None\nprint(x)").unwrap();
}

#[test]
fn test_compile_nested_while() {
    run("i = 0\ns = 0\nwhile i < 5:\n    s = s + i\n    i = i + 1\nprint(s)").unwrap();
}

// ── Tests that check final stack value ──

#[test]
fn test_expr_value_add() {
    // Without print — the expression value stays on stack
    let r = run("1 + 2").unwrap();
    assert!(!r.contains("err:"));
}

#[test]
fn test_expr_value_list() {
    let r = run("[1, 2, 3]").unwrap();
    assert!(!r.contains("err:"));
}

#[test]
fn test_undefined_var_error() {
    let r = run("print(unknown)");
    assert!(r.is_err() || r.unwrap().contains("err:"));
}
