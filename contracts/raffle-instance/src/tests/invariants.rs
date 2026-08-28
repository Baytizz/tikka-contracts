//! Escrow solvency, prize-sum invariants, and resource cost ceilings.

use super::*;

#[test]
fn get_my_tickets_cost_stays_below_ceiling_for_10k_owned_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, _contract_id, buyer, _, _, _) =
        setup_scale_raffle(&env, 10_000, 1_000, MIN_TICKET_PRICE * 20);
    for _ in 0..10 {
        client.buy_tickets(&buyer, &1_000);
    }

    let (cpu, mem) = record_costs(&env, || {
        let tickets = client.get_my_tickets(&buyer);
        assert_eq!(tickets.len(), 10_000);
    });

    assert!(
        cpu < GET_MY_TICKETS_10K_CPU_CEILING,
        "get_my_tickets 10k CPU {cpu} exceeded ceiling {GET_MY_TICKETS_10K_CPU_CEILING}"
    );
    assert!(
        mem < GET_MY_TICKETS_10K_MEM_CEILING,
        "get_my_tickets 10k memory {mem} exceeded ceiling {GET_MY_TICKETS_10K_MEM_CEILING}"
    );
}

fn setup_active_raffle(
    env: &Env,
) -> (
    ContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'_>,
) {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);

    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let (token_addr, token_mint) = create_token(env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "ticket sales pause"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr,
        prize_amount: MIN_TICKET_PRICE * 100,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
            metadata_hash: BytesN::from_array(env, &[7u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    (client, admin, creator, buyer, factory, token_mint)
}

#[test]
fn prize_distribution_invariant_holds_for_multiple_tiers() {
    let tier_configs: [[u32; 3]; 3] = [[10000, 0, 0], [5000, 5000, 0], [6000, 3000, 1000]];
    let fee_bps = [0u32, 100, 250, 1000, 2000];

    for tiers_raw in tier_configs {
        let tiers_count = if tiers_raw[2] > 0 {
            3
        } else if tiers_raw[1] > 0 {
            2
        } else {
            1
        };

        for fee_bp in fee_bps {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().set_timestamp(1_000);

            let factory = Address::generate(&env);
            let admin = Address::generate(&env);
            let creator = Address::generate(&env);
            let treasury = Address::generate(&env);
            let buyer_a = Address::generate(&env);
            let buyer_b = Address::generate(&env);
            let buyer_c = Address::generate(&env);

            let token_admin = Address::generate(&env);
            let payment_token = env
                .register_stellar_asset_contract_v2(token_admin.clone())
                .address();
            let token_client = StellarAssetClient::new(&env, &payment_token);
            token_client.mint(&creator, &10_000_000);
            token_client.mint(&buyer_a, &10_000_000);
            token_client.mint(&buyer_b, &10_000_000);
            token_client.mint(&buyer_c, &10_000_000);

            let contract_id = env.register(Contract, ());
            let client = ContractClient::new(&env, &contract_id);

            let prize_amount: i128 = 1_000_000;
            let ticket_price: i128 = MIN_TICKET_PRICE;
            let tickets_to_sell: u32 = tiers_count;
            let total_ticket_sales = ticket_price * tickets_to_sell as i128;
            let expected_ticket_fees = total_ticket_sales * fee_bp as i128 / 10_000;

            let prizes = match tiers_count {
                1 => soroban_sdk::vec![&env, tiers_raw[0]],
                2 => soroban_sdk::vec![&env, tiers_raw[0], tiers_raw[1]],
                _ => soroban_sdk::vec![&env, tiers_raw[0], tiers_raw[1], tiers_raw[2]],
            };

            let config = RaffleConfig {
                description: String::from_str(&env, "Prize invariant"),
                end_time: 0,
                no_deadline: true,
                max_tickets: tickets_to_sell,
                max_tickets_per_tx: tickets_to_sell,
                min_tickets: 1,
                allow_multiple: true,
                ticket_price,
                payment_token: payment_token.clone(),
                prize_amount,
                prizes,
                randomness_source: RandomnessSource::Internal,
                oracle_address: None,
                protocol_fee_bp: fee_bp,
                treasury_address: Some(treasury.clone()),
                swap_router: None,
                tikka_token: None,
                unique_winners: false,
            metadata_hash: BytesN::from_array(&env, &[33; 32]),
                claim_lockup_seconds: 0,
                swap_deadline_seconds: 0,
            };

            client.init(&factory, &admin, &creator, &config);
            client.deposit_prize();

            client.buy_tickets(&buyer_a, &1);
            if tickets_to_sell > 1 {
                client.buy_tickets(&buyer_b, &1);
            }
            if tickets_to_sell > 2 {
                client.buy_tickets(&buyer_c, &1);
            }

            client.finalize_raffle();

            let token = soroban_sdk::token::Client::new(&env, &payment_token);
            let contract_balance_before_claims = token.balance(&contract_id);

            env.ledger()
                .set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);

            let raffle = client.get_raffle();
            let mut total_claimed = 0i128;
            for i in 0..raffle.winners.len() {
                let winner = raffle.winners.get(i).unwrap();
            let mut fee_from_prize = 0i128;
            let winners = raffle.winners;
            for tier_idx in 0..tiers_count {
                let amt = client.claim_prize(&winners.get(tier_idx as u32).unwrap(), &(tier_idx as u32));
                total_claimed += amt;
                let tier_fee = (amt * fee_bp as i128 + 9999) / 10_000;
                fee_from_prize += tier_fee;
            }

            let leftover = prize_amount - total_claimed;
            assert_eq!(leftover, 0);
            
            assert_eq!(client.get_accumulated_fees(), expected_ticket_fees + fee_from_prize);
            assert_eq!(token.balance(&treasury), expected_ticket_fees);

            let contract_balance_after_claims = token.balance(&contract_id);
            assert_eq!(
                contract_balance_after_claims,
                contract_balance_before_claims - prize_amount
            );
        }
    }
}

#[test]
fn finalize_raffle_cost_stays_below_ceiling_for_10k_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, contract_id, buyer, _, _, _) =
        setup_scale_raffle(&env, 10_000, 1_000, MIN_TICKET_PRICE * 20);
    for _ in 0..10 {
        client.buy_tickets(&buyer, &1_000);
    }

    let (cpu, mem) = record_costs(&env, || {
        client.finalize_raffle();
    });

    assert!(
        cpu < FINALIZE_10K_CPU_CEILING,
        "finalize_raffle 10k CPU {cpu} exceeded ceiling {FINALIZE_10K_CPU_CEILING}"
    );
    assert!(
        mem < FINALIZE_10K_MEM_CEILING,
        "finalize_raffle 10k memory {mem} exceeded ceiling {FINALIZE_10K_MEM_CEILING}"
    );

    env.as_contract(&contract_id, || {
        let raffle = crate::read_raffle(&env).unwrap();
        assert_eq!(raffle.status, RaffleStatus::Finalized);
    });
}
