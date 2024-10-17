use std::default;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    #[default]
    BinanceSpot,
    CoinbaseSpot,
    OkxSpot,
    KrakenSpot,
}

impl Exchange {
    pub fn from_str(s: &str) -> Option<Exchange> {
        match s {
            "BinanceSpot" => Some(Exchange::BinanceSpot),
            "CoinbaseSpot" => Some(Exchange::CoinbaseSpot),
            "OkxSpot" => Some(Exchange::OkxSpot),
            "KrakenSpot" => Some(Exchange::KrakenSpot),
            _ => None,
        }
    }
}

// version1.0 only support limit order
#[derive(Debug, Default, Serialize, Deserialize)]
pub enum OrderType {
    #[default]
    Limit,
    Market,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub enum TimeInForce {
    #[default]
    Gtc,
    Ioc,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub enum OrderSide {
    #[default]
    Buy,
    Sell,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub enum OrderState {
    #[default]
    Open,
    Canceled,
    PartiallyFilled,
    Filled,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Balance {
    total: f64,
    available: f64,
    freezed: f64,
}

impl Balance {
    pub fn add_freezed(&mut self, value: f64) {
        self.freezed += value;
        self.available -= value;
    }

    pub fn sub_freezed(&mut self, value: f64) {
        self.freezed -= value;
        self.available += value;
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FeeRate {
    maker_fee: f64,
    taker_fee: f64,
}

// version1.0 only support spot position, so no position side
#[derive(Debug, Default)]
pub enum PositionSide {
    #[default]
    Long,
    Short,
}

#[derive(Debug, Default)]
pub struct Account {
    pub backtest_id: String,
    pub balance: Balance,
    pub position: Position,
}

#[derive(Debug, Default)]
pub struct Position {
    exchange: Exchange,
    symbol: String,
    // side: PositionSide,
    amount: f64,
    entry_price: f64,
    // entry_time: i64,
}

impl Position {
    pub fn update_pos(&mut self, price: f64, amount: f64) {
        let old_value = self.amount * self.entry_price;
        self.amount += amount;
        self.entry_price = (old_value + amount * price) / self.amount;
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Order {
    pub exchange: Exchange,
    pub cid: String,
    pub symbol: String,
    pub price: f64,
    pub amount: f64,
    pub filed_amount: f64,
    pub avg_price: f64,
    pub side: OrderSide,
    pub state: OrderState,
    pub order_type: OrderType,
    pub time_in_force: TimeInForce,
    pub timestamp: i64,
}

impl Order {
    pub fn execute(&mut self, depth: &Depth) -> (f64, f64) {
        let mut avg_price = 0.0;
        let mut executed_amount= 0.0;
        let mut executed_value = 0.0;
        let mut rest_amount = self.amount - self.filed_amount;
        let mut pos_coefficient = 1.0;
        match self.side {
            OrderSide::Buy => {
                for ask in depth.asks.iter() {
                    if ask.price > self.price || rest_amount <= 0.0{
                        break;
                    }
                    let amount_to_execute = rest_amount.min(ask.amount);
                    executed_amount += amount_to_execute;
                    executed_value += amount_to_execute * ask.price;
                    avg_price = executed_value / executed_amount;
                    rest_amount -= amount_to_execute;
                }
            }
            OrderSide::Sell => {
                for bid in depth.bids.iter() {
                    if bid.price < self.price || rest_amount <= 0.0{
                        break;
                    }
                    let amount_to_execute = rest_amount.min(bid.amount);
                    executed_amount += amount_to_execute;
                    executed_value += amount_to_execute * bid.price;
                    avg_price = executed_value / executed_amount;
                    rest_amount -= amount_to_execute;
                }
                pos_coefficient = -1.0;
            }
        }
        
        self.filed_amount = executed_amount;
        self.avg_price = avg_price;

        if self.filed_amount == self.amount {
            self.state = OrderState::Filled;
        } else if self.filed_amount > 0.0 {
            self.state = OrderState::PartiallyFilled;
        }
            
        (avg_price, executed_amount * pos_coefficient)
    }
}

#[derive(Debug)]
pub struct Level {
    price: f64,
    amount: f64,
}

#[derive(Default)]
pub struct Depth {
    pub symbol: String,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
    pub timestamp: i64,
}

#[derive(Default)]
pub struct Trade;
