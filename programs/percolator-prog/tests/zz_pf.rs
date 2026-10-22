use percolator_prog::state;
#[test]
fn pf() { let d=std::fs::read("/tmp/pf.bin").unwrap(); let p=state::read_portfolio(&d).unwrap();
 let l=&p.legs[0]; println!("LEG {:?}", l);
 let mid=l.market_id; let b=mid.to_le_bytes(); let off=d.windows(8).position(|w| w==b); println!("market_id={} off={:?}", mid, off);
 let bq=(l.basis_pos_q as i128).to_le_bytes(); println!("basis={} off={:?}", l.basis_pos_q, d.windows(16).position(|w| w==bq)); }
