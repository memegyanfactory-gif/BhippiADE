//! Structural guard for ADR-0036's contract-only boundary.

#[test]
fn runtime_contract_has_no_path_network_or_authored_document_authority() {
    let source = include_str!("../src/runtime_contract.rs");
    for forbidden in [
        "std::path",
        "PathBuf",
        "std::fs",
        "TcpStream",
        "UdpSocket",
        "reqwest",
        "SceneDocument",
        "EngineTransaction",
        "apply_transaction",
    ] {
        assert!(
            !source.contains(forbidden),
            "runtime contract must not contain ambient/authored authority: {forbidden}"
        );
    }
}

#[test]
fn runtime_contract_is_registry_bound_and_contract_only() {
    let source = include_str!("../src/runtime_contract.rs");
    assert!(source.contains("CapabilityRegistry"));
    assert!(source.contains("capabilities.describe(capability)"));
    assert!(!source.contains("pub fn tick("));
    assert!(!source.contains("pub fn load("));
    assert!(!source.contains("pub fn run_job("));
}
