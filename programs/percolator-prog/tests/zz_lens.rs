use percolator_prog::state;
#[test]
fn lens() { println!("MARKET_LEN={} PORTFOLIO_LEN={}", state::market_account_len_for_capacity(1).unwrap(), state::portfolio_account_len_for_market_slots(1).unwrap()); }
