//! Exhaustive status × entrypoint matrix (#832).

use raffle_shared::{CancelReason, RaffleConfig, RaffleStatus, RandomnessSource};
use soroban_sdk::{token::StellarAssetClient, Address, BytesN, Env, String, Vec};

use crate::{
    read_raffle, write_raffle, DataKey, Error, RaffleInstance, RaffleInstanceClient, MIN_TICKET_PRICE,
};

#[derive(Clone, Copy, Debug)]
enum Entrypoint {
    DepositPrize,
    BuyTickets,
    FinalizeRaffle,
    ClaimPrize,
    SweepUnclaimed,
    CancelRaffle,
    RefundPrize,
    PreviewBuy,
}

fn seed_raffle(env: &Env, contract_id: &Address, status: RaffleStatus) {
    env.as_contract(contract_id, || {
        let mut raffle = read_raffle(env).unwrap_or_else(|_| {
            panic!("raffle must be initialised before matrix tests");
        });
        raffle.status = status;
        write_raffle(env, &raffle);
    });
}

fn setup_pending(env: &Env) -> (RaffleInstanceClient<'_>, Address, Address, Address) {
    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);
    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let token_admin = Address::generate(env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    StellarAssetClient::new(env, &payment_token).mint(&creator, &1_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "matrix"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 3,
        max_tickets_per_tx: 3,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token,
        prize_amount: MIN_TICKET_PRICE * 3,
        prizes: Vec::from_array(env, [10_000u32]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[1u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    (client, contract_id, creator, admin)
}

fn invoke(
    client: &RaffleInstanceClient<'_>,
    creator: &Address,
    entrypoint: Entrypoint,
) -> Result<(), Error> {
    match entrypoint {
        Entrypoint::DepositPrize => client.try_deposit_prize().map(|_| ()),
        Entrypoint::BuyTickets => client
            .try_buy_tickets(creator, &1u32)
            .map(|_| ())
            .map_err(|e| e.unwrap()),
        Entrypoint::FinalizeRaffle => client.try_finalize_raffle().map(|_| ()),
        Entrypoint::ClaimPrize => client
            .try_claim_prize(creator, &0u32)
            .map(|_| ())
            .map_err(|e| e.unwrap()),
        Entrypoint::SweepUnclaimed => client.try_sweep_unclaimed().map(|_| ()),
        Entrypoint::CancelRaffle => client
            .try_cancel_raffle(&CancelReason::CreatorCancelled)
            .map(|_| ()),
        Entrypoint::RefundPrize => client.try_refund_prize().map(|_| ()),
        Entrypoint::PreviewBuy => client.try_preview_buy(&1u32).map(|_| ()),
    }
}

fn expected_illegal(status: RaffleStatus, entrypoint: Entrypoint) -> bool {
    use Entrypoint::*;
    use RaffleStatus::*;
    match (status, entrypoint) {
        (PendingPrize, DepositPrize) => false,
        (PendingPrize, PreviewBuy) => false,
        (Active, BuyTickets) => false,
        (Active, FinalizeRaffle) => false,
        (Active, CancelRaffle) => false,
        (Finalized, ClaimPrize) => false,
        (Finalized, SweepUnclaimed) => false,
        (Cancelled | Failed, RefundPrize) => false,
        (Claimed | Cancelled | Failed, _) if matches!(entrypoint, DepositPrize | BuyTickets | FinalizeRaffle | CancelRaffle) => true,
        (PendingPrize, BuyTickets | FinalizeRaffle | ClaimPrize | SweepUnclaimed | CancelRaffle | RefundPrize) => true,
        (Active, DepositPrize | ClaimPrize | SweepUnclaimed | RefundPrize) => true,
        (Drawing, _) => true,
        (Finalized, DepositPrize | BuyTickets | FinalizeRaffle | CancelRaffle | RefundPrize) => true,
        (Claimed, _) => true,
        _ => true,
    }
}

#[test]
fn terminal_states_are_absorbing() {
    for status in RaffleStatus::all() {
        assert_eq!(status.is_terminal(), matches!(status, RaffleStatus::Cancelled | RaffleStatus::Failed | RaffleStatus::Claimed));
        if status.is_terminal() {
            for target in RaffleStatus::all() {
                if *target != status {
                    assert!(
                        !status.can_transition_to(*target),
                        "{status:?} must not transition to {target:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn transition_graph_matches_canonical_lifecycle() {
    assert!(RaffleStatus::PendingPrize.can_transition_to(RaffleStatus::Active));
    assert!(RaffleStatus::Active.can_transition_to(RaffleStatus::Drawing));
    assert!(RaffleStatus::Active.can_transition_to(RaffleStatus::Failed));
    assert!(RaffleStatus::Active.can_transition_to(RaffleStatus::Cancelled));
    assert!(RaffleStatus::Drawing.can_transition_to(RaffleStatus::Finalized));
    assert!(RaffleStatus::Drawing.can_transition_to(RaffleStatus::Cancelled));
    assert!(RaffleStatus::Finalized.can_transition_to(RaffleStatus::Claimed));
    assert!(RaffleStatus::Drawing.can_internal_revert_to(RaffleStatus::Active));
    assert!(!RaffleStatus::Active.can_internal_revert_to(RaffleStatus::Drawing));
}

#[test]
fn state_entrypoint_matrix_documents_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let entrypoints = [
        Entrypoint::DepositPrize,
        Entrypoint::BuyTickets,
        Entrypoint::FinalizeRaffle,
        Entrypoint::ClaimPrize,
        Entrypoint::SweepUnclaimed,
        Entrypoint::CancelRaffle,
        Entrypoint::RefundPrize,
        Entrypoint::PreviewBuy,
    ];

    for status in RaffleStatus::all() {
        let (client, contract_id, creator, _) = setup_pending(&env);
        seed_raffle(&env, &contract_id, *status);

        for entrypoint in entrypoints {
            let result = invoke(&client, &creator, entrypoint);
            if expected_illegal(*status, entrypoint) {
                assert!(
                    result.is_err(),
                    "{status:?} + {entrypoint:?} should be rejected"
                );
                if let Err(err) = result {
                    assert!(
                        matches!(
                            err,
                            Error::InvalidStatus
                                | Error::InvalidStateTransition
                                | Error::RaffleInactive
                                | Error::PrizeAlreadyDeposited
                                | Error::PrizeNotDeposited
                                | Error::NotInitialized
                                | Error::InvalidParameters
                                | Error::ClaimTooEarly
                        ),
                        "{status:?} + {entrypoint:?} returned unexpected {err:?}"
                    );
                }
            }
        }
    }
}
