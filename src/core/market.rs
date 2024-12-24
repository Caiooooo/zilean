use crate::engine::*;
use clickhouse::Row;
use log::info;
use serde::ser::Serializer;
use sonic_rs::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    #[default]
    BinanceSpot,
    CoinbaseSpot,
    OkxSpot,
    KrakenSpot,
    BinanceSwap,
    BybitSwap,
    BinanceSwapDec,
    BinanceSwapInc,
    BitgetSwap,
}

// version1.0 only support limit order
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum OrderType {
    #[default]
    Limit,
    Market,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum TimeInForce {
    #[default]
    Gtc,
    Ioc,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub enum OrderSide {
    #[default]
    Buy,
    Sell,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub enum OrderState {
    #[default]
    Open,
    Canceled,
    PartiallyFilled,
    Filled,
}
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub enum ContractType {
    #[default]
    Spot, // 现货
    Futures,    // 期货(永续合约)
    // Options,    // 期权
    // Perpetual, // 永续合约
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum MarginMode {
    Cross, // 全仓
    #[default]
    Isolated, // 逐仓
}

// 用于平仓
pub struct ClosePosition {
    pub symbol: String,
    pub amount: f64,
    pub price: f64,
    pub side: OrderSide,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Balance {
    total: f64,
    available: f64,
    freezed: f64,
    // #[serde(skip_serializing)]
    // maker_fee: f64, // to add when stable in the future
}

impl Balance {
    pub fn get_available(&self) -> f64 {
        self.available
    }

    pub fn add_freezed(&mut self, value: f64) {
        self.freezed += value;
        self.available -= value;
    }

    pub fn sub_freezed(&mut self, value: f64) {
        self.freezed -= value;
        self.available += value;
    }
    pub fn fill_freezed(&mut self, filled: &FilledStack){
        match filled.contract_type {
            ContractType::Spot => self.fill_freezed_spot(filled),
            ContractType::Futures => self.fill_freezed_futures(filled),
        }
    }
    pub fn fill_freezed_futures(&mut self, filled: &FilledStack) {
        info!("bal filled: {:?}, before filled: t{}, a{}, f{}", filled, self.total, self.available, self.freezed);
        let amount = filled.filled_amount;
        let value = ((filled.filled_price * filled.filled_amount / filled.leverage as f64) * 1e12).round()/1e12;
        let freeze = ((filled.post_price * filled.filled_amount / filled.leverage as f64) * 1e12).round()/1e12;
        let value_open = (filled.open_price.unwrap_or(filled.filled_price) * filled.filled_amount / filled.leverage as f64 * 1e12).round()/1e12;
        if filled.side == PositionSide::Long {
            // open long:amount>0  / close long: amount<0 
            if amount > 0.0 {
                // println!("value: {}, freeze: {}, available: {}, lev: {}", value, freeze, self.available, filled.leverage);
                self.total -= value ;
                // println!("filled.freeze_mar: {}, amount: {}, amount_tol: {}", filled.freeze_margin, amount, filled.amount_total);
                // println!("fill:{:?}", filled);
                // println!("subFreeze,{}", filled.freeze_margin * (amount / filled.amount_total));
                self.freezed -= freeze;
                self.available += freeze - value ;
            } else {
                // println!("fill: {}, open: {}, available: {}, lev: {}", filled.filled_price, filled.open_price.unwrap_or(filled.filled_price), self.available, filled.leverage);
                self.total -= value_open ;
                self.available -= value_open ;
                
                self.total += filled.filled_amount * (filled.filled_price - filled.open_price.unwrap_or(filled.filled_price));
                self.available += filled.filled_amount * (filled.filled_price - filled.open_price.unwrap_or(filled.filled_price));
                // println!("value: {}, freeze: {}", value * (filled.leverage - 1) as f64, value * filled.leverage as f64);
            }
        } else if filled.side == PositionSide::Short {
            // open short:amount<0  / close short: amount>0 
            if amount < 0.0 {
                self.total += value ;
                self.freezed += freeze;
                self.available -= freeze - value ;
            } else {
                self.total += value_open ;
                self.available += value_open ;
                
                self.total -= filled.filled_amount * (filled.filled_price - filled.open_price.unwrap_or(filled.filled_price));
                self.available -= filled.filled_amount * (filled.filled_price - filled.open_price.unwrap_or(filled.filled_price));
            }
        }
        info!("bal filled: {:?}, after filled: t{}, a{}, f{}", filled.cid, self.total, self.available, self.freezed);
        self.round();
        // self.freezed -= filled.freeze_margin * (amount / filled.amount_total);
        // // buy futures, amount > 0, available increase
        // // open long \ close shot \ open shot \ close long
        // self.available += filled.freeze_margin * (amount / filled.amount_total) - price * amount;
    }
    pub fn fill_freezed_spot(&mut self, filled: &FilledStack) {
        // buy coin, amount > 0
        // debug!("fill_freezed: price: {}, amount: {}, freeze_price: {}", price, amount, freeze_price);
        let price = filled.filled_price;
        let amount = filled.filled_amount;
        let freeze_price = filled.post_price;
        self.total -= price * amount;
        // buy coin, amount > 0, available increase
        if amount > 0.0 {
            self.freezed -= freeze_price * amount;
            self.available += (freeze_price - price) * amount;
        } else {
            self.available -= price * amount;
        }
    }

    pub fn round(&mut self) {
        self.freezed = (self.freezed * 1e12).round() / 1e12;
        self.available = (self.available * 1e12).round() / 1e12;
        self.total = (self.total * 1e12).round() / 1e12;
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct FeeRate {
    maker_fee: f64,
    taker_fee: f64,
}

// version1.0 only support spot position, so no position side
#[derive(Debug, Default, Serialize, Clone, Hash, Eq, PartialEq, Deserialize)]
pub enum PositionSide {
    #[default]
    Long,
    Short,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct Account {
    pub backtest_id: String,
    pub balance: Balance,
    #[serde(serialize_with = "serialize_position_map")]
    pub position: HashMap<(String, PositionSide, Exchange), Position>, // String: Symbol_ContractType
}
fn serialize_position_map<S>(
    position_map: &HashMap<(String, PositionSide, Exchange), Position>, 
    serializer: S
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map: HashMap<String, Vec<Position>> = HashMap::new();

    // Iterate over position_map and group positions by symbol
    for ((symbol, _side, _exchange), position) in position_map {
        // Insert the position into the Vec associated with the symbol key
        map.entry(symbol.clone())
            .or_default()
            .push(position.clone());  // Add the position to the vector
    }

    // Serialize the resulting map
    map.serialize(serializer)
}


#[derive(Debug, Default, Serialize, Clone)]
// only support one exchange and one symbol
pub struct Position {
    pub side: PositionSide,
    pub exchange: Exchange,
    pub leverage: u32,
    pub margin_value: f64,
    pub amount_total: f64,
    pub amount_available: f64,
    pub amount_freezed: f64,
    pub entry_price: f64,
    // entry_time: i64,
    pub stop_loss: Vec<(f64, f64)>,
}

impl Position {
    pub fn update_pos(&mut self, filled : &FilledStack) {
        self.round();
        info!("filled: {:?}, before filled: t{}, a{}, f{}", filled.cid, self.amount_total, self.amount_available, self.amount_freezed);
        let price = filled.filled_price;
        let amount = filled.filled_amount;
        let old_value = self.amount_total * self.entry_price;

        if filled.contract_type == ContractType::Futures {
            if filled.side == PositionSide::Long {
                // open long:amount>0 / close long: amount<0
                self.amount_total += amount;
                if amount>0.0{ // open long
                    if self.amount_total == 0.0 {
                        self.entry_price = 0.0;
                    } else {
                        self.entry_price = (old_value + amount.abs() * price) / self.amount_total;
                    }
                    self.amount_available += amount;
                }else{ // close long
                    self.amount_freezed += amount;
                }
            }else if filled.side == PositionSide::Short {
                // println!("{:?}", amount);
                self.amount_total -= amount;
                if amount<0.0{ // open short
                    if self.amount_total == 0.0 {
                        self.entry_price = 0.0;
                    } else {
                        self.entry_price = (old_value + amount.abs() * price) / self.amount_total;
                    }
                    self.amount_available -= amount;
                }else{ // close short
                    self.amount_freezed -= amount;
                }
            }
            if self.amount_total == 0.0 {
                self.entry_price = 0.0;
            }
            
            self.margin_value = self.amount_total * self.entry_price / filled.leverage as f64;
        }else if filled.contract_type == ContractType::Spot {
            self.amount_total += amount;
            if amount > 0.0 {
                self.amount_available += amount;
            } else {
                self.amount_freezed += amount;
            }

            if self.amount_total == 0.0 {
                self.entry_price = 0.0;
            } else {
                self.entry_price = (old_value + amount * price) / self.amount_total;
            }
        }
        info!("pos filled: {:?}, after filled: t{}, a{}, f{}", filled.cid, self.amount_total, self.amount_available, self.amount_freezed);
    }

    pub fn add_freezed(&mut self, value: f64) {
        self.amount_freezed += value;
        self.amount_available -= value;
    }

    pub fn sub_freezed(&mut self, value: f64) {
        self.amount_freezed -= value;
        self.amount_available += value;
    }

    // buy order fill, value > 0
    // pub fn fill_freezed(&mut self, value: f64) {
        
    // }

    pub fn round(&mut self) {
        self.amount_freezed = (self.amount_freezed * 1e6).round() / 1e6;
        self.amount_available = (self.amount_available * 1e6).round() / 1e6;
        self.amount_total = (self.amount_total * 1e6).round() / 1e6;
        if self.entry_price.is_nan() || self.entry_price.is_infinite() {
            self.entry_price = 0.0;
        }
    }
}

// skip: ignore this field when serialize, single exchange don't need this field
// default on spot
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Order {
    // for futures
    #[serde(default, skip_serializing)]
    pub contract_type: ContractType,
    #[serde(default)]
    pub position_side: PositionSide,
    // New fields for contract margin mode and leverage
    #[serde(default, skip_serializing)]
    pub margin_mode: MarginMode, // Margin mode: Cross or Isolated
    #[serde(default = "default_leverage", skip_serializing)]
    pub leverage: u32, // set leverage multiplier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>, // Take profit price
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>, // Stop loss price
    #[serde(default, skip_serializing)]
    pub reduce_only: bool, // default false

    #[serde(skip_serializing)]
    pub exchange: Exchange,
    #[serde(skip_serializing)]
    pub open_price: Option<f64>, // avg_price
    pub cid: String,
    #[serde(skip_serializing)]
    pub symbol: String,
    pub price: f64,
    pub amount: f64,
    pub filled_amount: f64,
    #[serde(default = "default_amount", skip)]
    pub front_amount: f64,
    #[serde(default = "default_depth", skip)]
    pub prev_depth: Depth,
    pub avg_price: f64,
    pub side: OrderSide,
    pub state: OrderState,
    #[serde(skip_serializing)]
    pub order_type: OrderType,
    #[serde(skip)]
    pub margin: f64,
    #[serde(skip_serializing)]
    pub time_in_force: TimeInForce,
    #[serde(skip_serializing)]
    pub timestamp: i64,
}
fn default_leverage() -> u32 {
    1
}
fn default_amount() -> f64 {
    -1.0
}
fn default_depth() -> Depth {
    Depth::default()
}

impl Order {
    pub fn execute(&mut self, depth: &Depth, fill_model: FillModel) -> (f64, f64) {
        let mut avg_price = 0.0;
        let mut executed_amount = 0.0;
        let mut executed_value = 0.0;
        let mut rest_amount = self.amount - self.filled_amount;
        let mut pos_coefficient = 1.0;
        if (self.side == OrderSide::Buy && self.position_side == PositionSide::Long && self.contract_type == ContractType::Futures)
            || (self.side == OrderSide::Sell && self.position_side == PositionSide::Short && self.contract_type == ContractType::Futures)
                || (self.side == OrderSide::Buy && self.contract_type == ContractType::Spot){
                // Maintain an execution position queue
                if depth.asks[0].0 > self.price && rest_amount > 0.0 {
                    let exe_amount = self.update_front_amount(depth, fill_model);
                    if exe_amount != 0.0 {
                        executed_amount += exe_amount;
                        executed_value += exe_amount * self.price;
                        avg_price = executed_value / executed_amount;
                    }
                } else {
                    for ask in depth.asks.iter() {
                        if ask.0 > self.price || rest_amount <= 0.0 {
                            break;
                        }
                        self.front_amount = 0.0;
                        let amount_to_execute = rest_amount.min(ask.1);
                        executed_amount += amount_to_execute;
                        executed_value += amount_to_execute * ask.0;
                        avg_price = executed_value / executed_amount;
                        rest_amount -= amount_to_execute;
                    }
                }
        }else if (self.side == OrderSide::Buy && self.position_side == PositionSide::Short && self.contract_type == ContractType::Futures)
        || (self.side == OrderSide::Sell && self.position_side == PositionSide::Long && self.contract_type == ContractType::Futures)
        || (self.side == OrderSide::Sell && self.contract_type == ContractType::Spot)
        {
            if depth.bids[0].0 < self.price && rest_amount > 0.0 {
                let exe_amount = self.update_front_amount(depth, fill_model);
                if exe_amount != 0.0 {
                    executed_amount += exe_amount;
                    executed_value += exe_amount * self.price;
                    avg_price = executed_value / executed_amount;
                }
            } else {
                for bid in depth.bids.iter() {
                    if bid.0 < self.price || rest_amount <= 0.0 {
                        break;
                    }
                    let amount_to_execute = rest_amount.min(bid.1);
                    executed_amount += amount_to_execute;
                    executed_value += amount_to_execute * bid.0;
                    avg_price = executed_value / executed_amount;
                    rest_amount -= amount_to_execute;
                }
            }
            pos_coefficient = -1.0;
        }
                

        // this is out of most max precision of post, so it won't change the result
        executed_amount = (executed_amount * 1e6).round() / 1e6;
        avg_price = (avg_price * 1e12).round() / 1e12;
        if avg_price > 0.0 {
            self.avg_price = (avg_price * executed_amount + self.avg_price * self.filled_amount)
                / (self.filled_amount + executed_amount);
        }
        self.filled_amount += executed_amount;

        if self.filled_amount == self.amount {
            self.timestamp = depth.local_timestamp;
            self.state = OrderState::Filled;
        } else if self.filled_amount > 0.0 {
            self.state = OrderState::PartiallyFilled;
        }
        (avg_price, executed_amount * pos_coefficient)
    }

    // ------------- return amount to be filled -------------------
    #[allow(clippy::needless_return)]
    fn update_front_amount(&mut self, new_depth: &Depth, fill_model: FillModel) -> f64 {
        // init amount
        if self.front_amount < 0.0 {
            self.front_amount = 0.0;
            match self.side {
                OrderSide::Buy => {
                    for ask in new_depth.asks.iter() {
                        if ask.0 <= self.price {
                            self.front_amount = ask.1;
                            continue;
                        }
                        break;
                    }
                }
                OrderSide::Sell => {
                    for bid in new_depth.bids.iter() {
                        if bid.0 >= self.price {
                            self.front_amount = bid.1;
                            continue;
                        }
                        break;
                    }
                }
            }
            self.prev_depth = new_depth.clone();
            return 0.0;
        }
        let mut prev_amount = 0.0;
        let mut new_amount = 0.0;
        match self.side {
            OrderSide::Buy => {
                for ask in new_depth.asks.iter() {
                    if ask.0 <= self.price {
                        new_amount = ask.1;
                        continue;
                    }
                    break;
                }
                for ask in self.prev_depth.asks.iter() {
                    if ask.0 <= self.price {
                        prev_amount = ask.1;
                        continue;
                    }
                    break;
                }
            }
            OrderSide::Sell => {
                for bid in new_depth.bids.iter() {
                    if bid.0 >= self.price {
                        new_amount = bid.1;
                        continue;
                    }
                    break;
                }
                for bid in self.prev_depth.bids.iter() {
                    if bid.0 >= self.price {
                        prev_amount = bid.1;
                        continue;
                    }
                    break;
                }
            }
        };
        let chg = prev_amount - new_amount;
        if chg < 0.0 {
            self.front_amount = self.front_amount.min(new_amount);
            return 0.0;
        }

        let front = self.front_amount;
        let back = prev_amount - front;

        let mut prob = fill_model.prob(back, front);
        if prob > 1.0 || prob.is_infinite() {
            prob = 1.0;
        }
        let new_front = front - (1.0 - prob) * chg + (back - prob * chg).min(0.0);
        self.front_amount = new_front.min(new_amount).min(0.0);
        // match success on front amount update
        if self.front_amount == 0.0
            && ((self.side == OrderSide::Buy && (self.price - new_depth.bids[0].0).abs() < 1e-11)
                || (self.side == OrderSide::Sell
                    && (new_depth.asks[0].0 - self.price).abs() < 1e-11))
        {
            let mut ret = chg;

            if chg > self.amount - self.filled_amount {
                ret = self.amount - self.filled_amount;
            }
            ret = (ret * 1e6).round() / 1e6;
            return ret;
        }
        return 0.0;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Level {
    price: f64,
    amount: f64,
}

#[derive(Default, Deserialize, Serialize, Row, Debug, Clone)]
pub struct Depth {
    #[serde(deserialize_with = "deserialize_exchange")]
    pub exchange: Exchange,
    pub symbol: String,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub timestamp: i64,
    pub local_timestamp: i64,
}

fn deserialize_exchange<'de, D>(deserializer: D) -> Result<Exchange, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.as_str() {
        "binance" => Ok(Exchange::BinanceSpot),
        "coinbase" => Ok(Exchange::CoinbaseSpot),
        "kraken" => Ok(Exchange::KrakenSpot),
        "okx" => Ok(Exchange::OkxSpot),
        "okex" => Ok(Exchange::OkxSpot),
        "binance-futures" => Ok(Exchange::BinanceSwap),
        "BinanceSpot" => Ok(Exchange::BinanceSpot),
        "CoinbaseSpot" => Ok(Exchange::CoinbaseSpot),
        "KrakenSpot" => Ok(Exchange::KrakenSpot),
        "OkxSpot" => Ok(Exchange::OkxSpot),
        "OkexSpot" => Ok(Exchange::OkxSpot),
        "bybit" => Ok(Exchange::BybitSwap),
        "bitget_futures" => Ok(Exchange::BitgetSwap),
        "binance_swap" => Ok(Exchange::BinanceSwapDec),
        "bybit_swap" => Ok(Exchange::BybitSwap),
        "bitget_swap" => Ok(Exchange::BitgetSwap),
        // 更多匹配
        _ => Err(serde::de::Error::custom(format!("Unknown exchange: {}", s))),
    }
}

#[derive(Default, Debug, Deserialize, Serialize, Clone, Row)]
pub struct Trade {
    pub exchange: String,           
    pub symbol: String,               
    pub timestamp: i64,      
    pub local_timestamp: i64,        
    pub id: u64,                   
    pub price: f64,           
    pub amount: f64,          
    pub side: String,                 
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::*;
    use crate::engine::FillModel;

    #[test]
    fn test_order_execute() {
        // Buy Long (917.65 0.035344)
        // Buy Long (21716.0 0.035311)
        // Buy Long (102.0 0.035157)
        // Buy Long (153.0 0.034889)
        // Sell Long (22888.65 0.034886)
        let mut order = Order {
            contract_type: ContractType::Futures,
            position_side: PositionSide::Long,
            margin_mode: MarginMode::Cross,
            leverage: 10,
            exchange: Exchange::BinanceSwap,
            open_price: Some(100.0),
            cid: "test".to_string(),
            symbol: "btcusdt".to_string(),
            price: 100.0,
            amount: 10.0,
            filled_amount: 0.0,
            front_amount: -1.0,
            prev_depth: Depth::default(),
            avg_price: 0.0,
            side: OrderSide::Buy,
            state: OrderState::Open,
            order_type: OrderType::Limit,
            margin: 0.0,
            time_in_force: TimeInForce::Gtc,
            timestamp: 0,
            ..Default::default()
        };
        let depth = Depth {
            exchange: Exchange::BinanceSwap,
            symbol: "btcusdt".to_string(),
            bids: vec![(100.0, 10.0)],
            asks: vec![(100.0, 10.0)],
            timestamp: 0,
            local_timestamp: 0,
        };
        let fill_model = FillModel::None;
        let (avg_price, executed_amount) = order.execute(&depth, fill_model);
        assert_eq!(avg_price, 100.0);
        assert_eq!(executed_amount, 10.0);
        assert_eq!(order.filled_amount, 10.0);
        assert_eq!(order.state, OrderState::Filled);
    }

    #[test]
    fn test_position_update_pos() {
        let mut position = Position {
            side: PositionSide::Long,
            exchange: Exchange::BinanceSwap,
            leverage: 10,
            margin_value: 0.0,
            amount_total: 0.0,
            amount_available: 0.0,
            amount_freezed: 0.0,
            entry_price: 0.0,
            stop_loss: vec![(0.0, 0.0)],
        };
        let filled = FilledStack {
            exchange: Exchange::BinanceSwap,
            symbol: "btcusdt".to_string(),
            side: PositionSide::Long,
            contract_type: ContractType::Futures,
            leverage: 10,
            filled_price: 100.0,
            filled_amount: 10.0,
            post_price: 100.0,
            open_price: None,
            ..Default::default()
        };
        position.update_pos(&filled);
        assert_eq!(position.amount_total, 10.0);
        assert_eq!(position.amount_available, 10.0);
        assert_eq!(position.entry_price, 100.0);
        assert_eq!(position.margin_value, 100.0);
    }

    #[test]
    fn test_balance_update(){
        let balance = Balance{
            total: 100.0,
            available: 100.0,
            freezed: 0.0,
        };
        let mut account = Account{
            backtest_id: "test".to_string(),
            balance,
            position: HashMap::new(),
        };
        // let account = self
        //     .account
        //     .position
        //     .entry((filled.symbol.clone(), filled.side.clone(), filled.exchange))
        //     .or_default();
        // self.account.balance.fill_freezed(&filled);
        // account.update_pos(&filled);

        // Buy Long (917.65 0.035344)
        // Buy Long (21716.0 0.035311)
        // Buy Long (102.0 0.035157)
        // Buy Long (153.0 0.034889)
        // Sell Long (22888.65 0.034886)
        let buy1 = FilledStack {
            symbol: "btcusdt".to_string(),
            side: PositionSide::Long,
            contract_type: ContractType::Futures,
            leverage: 10,
            filled_price: 0.035344,
            filled_amount: 917.65,
            post_price: 0.035344,
            open_price: Some(0.035344),
            ..Default::default()
        };

        let buy2 = FilledStack {
            exchange: Exchange::BinanceSwap,
            symbol: "btcusdt".to_string(),
            side: PositionSide::Long,
            contract_type: ContractType::Futures,
            leverage: 10,
            filled_price: 0.035311,
            filled_amount: 21716.0,
            post_price: 0.035311,
            open_price: Some(0.035311),
            ..Default::default()
        };

        let buy3 = FilledStack {
            exchange: Exchange::BinanceSwap,
            symbol: "btcusdt".to_string(),
            side: PositionSide::Long,
            contract_type: ContractType::Futures,
            leverage: 10,
            filled_price: 0.035157,
            filled_amount: 102.0,
            post_price: 0.035157,
            open_price: Some(0.035157),
            ..Default::default()
        };

        let buy4 = FilledStack {
            exchange: Exchange::BinanceSwap,
            symbol: "btcusdt".to_string(),
            side: PositionSide::Long,
            contract_type: ContractType::Futures,
            leverage: 10,
            filled_price: 0.034889,
            filled_amount: 153.0,
            post_price: 0.034889,
            open_price: Some(0.034889),
            ..Default::default()
        };

        let sell = FilledStack {
            exchange: Exchange::BinanceSwap,
            symbol: "btcusdt".to_string(),
            side: PositionSide::Long,
            contract_type: ContractType::Futures,
            leverage: 10,
            filled_price: 0.034886,
            filled_amount: -22888.65,
            post_price: 0.034886,
            open_price: Some(0.035344),
            ..Default::default()
        };

        account.balance.fill_freezed_futures(&buy1);
        // println!("{:?}", balance);
        account.balance.fill_freezed_futures(&buy2);
        account.balance.fill_freezed_futures(&buy3);
        account.balance.fill_freezed_futures(&buy4);
        account.balance.fill_freezed_futures(&sell);
        // println!("{:?}", balance);

        let pos =  account
            .position
            .entry(("btcusdt".to_string(), PositionSide::Long, Exchange::BinanceSwap))
            .or_default();
        pos.update_pos(&buy1);
        pos.update_pos(&buy2);
        pos.update_pos(&buy3);
        pos.update_pos(&buy4);
        pos.update_pos(&sell);
        // println!("{:?}", pos);
        println!("{:?}", account);
        panic!();
    }
}