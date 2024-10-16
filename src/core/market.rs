#[derive(Debug)]
pub enum OrderType {
    Limit,
    Market,
}

pub enum TradeSide {
    Buy,
    Sell,
}

pub enum PositionSide {
    Long,
    Short,
}
pub enum LatencyModel {
    None,
    Fixed(i64),
    Random(i64, i64),
}

#[derive(Default)]
pub struct Order{
    price: f64,
    amount: f64
}

#[derive(Default)]
pub struct Position;

#[derive(Debug)]
pub struct Level {
    price: f64,
    amount: f64,
}

#[derive(Default)]
pub struct Depth{
    symbol: String,
    bids: Vec<Level>,
    asks: Vec<Level>,
    timestamp: i64,
}

#[derive(Default)]
pub struct Trade;
