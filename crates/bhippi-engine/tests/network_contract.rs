#![allow(clippy::expect_used)]

use bhippi_engine::network_contract::{
    AuthorityModel, IdentityContract, InputMessageContract, NetworkContract, NetworkContractError,
    NetworkContractLimits, NetworkTiming, PredictionContract, ReconciliationPolicy,
    ReplicationRule, ReplicationVisibility, RpcReliability, ServerRpcContract, TransportContract,
    NETWORK_CONTRACT_FORMAT,
};

fn fixture() -> NetworkContract {
    NetworkContract {
        format: NETWORK_CONTRACT_FORMAT.to_owned(),
        authority: AuthorityModel::DedicatedServer,
        identity: IdentityContract {
            server_issued: true,
            session_nonce_bytes: 16,
            entity_counter_bits: 64,
        },
        timing: NetworkTiming {
            simulation_hz: 60,
            snapshot_hz: 20,
            interpolation_millis: 100,
            maximum_rollback_ticks: 120,
        },
        transport: TransportContract {
            encryption_required: true,
            peer_authentication_required: true,
            reliable_ordered: true,
            unreliable_sequenced: true,
            maximum_payload_bytes: 16_384,
        },
        replication: vec![ReplicationRule {
            id: "player.transform".to_owned(),
            component: "Transform".to_owned(),
            fields: vec!["position".to_owned(), "rotation".to_owned()],
            visibility: ReplicationVisibility::RelevantClients,
            frequency_hz: 20,
        }],
        input_messages: vec![InputMessageContract {
            id: "player.input".to_owned(),
            actions: vec!["move".to_owned(), "jump".to_owned()],
            maximum_payload_bytes: 128,
            maximum_per_second: 120,
        }],
        server_rpcs: vec![ServerRpcContract {
            id: "interaction.request".to_owned(),
            permission: "gameplay.interact".to_owned(),
            reliability: RpcReliability::ReliableOrdered,
            maximum_payload_bytes: 256,
            maximum_per_second: 10,
        }],
        prediction: PredictionContract {
            predicted_components: vec!["Transform".to_owned()],
            maximum_history_ticks: 60,
            reconciliation: ReconciliationPolicy::AuthoritativeReplayInputs,
        },
    }
}

#[test]
fn bounded_server_authoritative_contract_validates_and_hashes_stably() {
    let first = fixture();
    first
        .validate(&NetworkContractLimits::default())
        .expect("fixture valid");
    let mut second = first.clone();
    second.server_rpcs.reverse();
    second.replication.reverse();
    assert_eq!(
        first.canonical_hash().expect("first hashes"),
        second.canonical_hash().expect("second hashes")
    );
}

#[test]
fn client_identity_and_insecure_transport_fail_closed() {
    let mut client_ids = fixture();
    client_ids.identity.server_issued = false;
    assert_eq!(
        client_ids.validate(&NetworkContractLimits::default()),
        Err(NetworkContractError::ClientIssuedIdentity)
    );

    let mut insecure = fixture();
    insecure.transport.encryption_required = false;
    assert_eq!(
        insecure.validate(&NetworkContractLimits::default()),
        Err(NetworkContractError::InsecureTransport)
    );
}

#[test]
fn payload_rate_prediction_and_duplicate_rules_are_bounded() {
    let limits = NetworkContractLimits::default();
    let mut flood = fixture();
    flood.input_messages[0].maximum_per_second = limits.maximum_messages_per_second + 1;
    assert_eq!(
        flood.validate(&limits),
        Err(NetworkContractError::OutsideLimit("message_rate"))
    );

    let mut prediction = fixture();
    prediction.prediction.predicted_components = vec!["Inventory".to_owned()];
    assert_eq!(
        prediction.validate(&limits),
        Err(NetworkContractError::PredictionWithoutReplication(
            "Inventory".to_owned()
        ))
    );

    let mut duplicate = fixture();
    duplicate.server_rpcs[0].id = duplicate.replication[0].id.clone();
    assert!(matches!(
        duplicate.validate(&limits),
        Err(NetworkContractError::DuplicateId(_))
    ));
}
