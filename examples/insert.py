#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ClickHouse 数据插入脚本
用于将 orderbook_btc_spot_okx_2024_10_07.parquet 数据插入到 ClickHouse
目标数据库: okx 和 okx_futures
表结构兼容 Rust Depth 结构体反序列化
"""

import pyarrow.parquet as pq
import pandas as pd
import json
import requests
from typing import List, Dict, Any
import numpy as np
import time


class ClickHouseInserter:
    def __init__(self, host: str = "http://localhost:8123", username: str = "default", password: str = "RyQplVJetI2M"):
        self.host = host
        self.username = username
        self.password = password
        self.session = requests.Session()
        self.session.auth = (username, password)

    def execute_sql(self, sql: str, data: str = None):
        """执行 ClickHouse SQL 语句"""
        url = f"{self.host}/"
        headers = {'Content-Type': 'text/plain'}
        
        if data:
            response = self.session.post(url, headers=headers, params={'query': sql}, data=data)
        else:
            response = self.session.post(url, headers=headers, data=sql)
        
        if response.status_code != 200:
            raise Exception(f"SQL执行失败: {response.text}")
        
        return response.text

    def create_databases(self):
        """创建数据库"""
        databases = ["okx", "okx_futures"]
        
        for db in databases:
            sql = f"CREATE DATABASE IF NOT EXISTS {db}"
            self.execute_sql(sql)

    def create_orderbook_table(self, database: str, symbol: str = "btcusdt"):
        """
        创建 orderbook 表，兼容 Rust Depth 结构体
        
        Rust Depth 结构体字段:
        - exchange: Exchange (String)
        - symbol: String  
        - bids: Vec<(f64, f64)> -> Array(Tuple(Float64, Float64))
        - asks: Vec<(f64, f64)> -> Array(Tuple(Float64, Float64))  
        - timestamp: i64
        - local_timestamp: i64
        """
        table_name = f"orderbook_{symbol}"
        
        sql = f"""
        CREATE TABLE IF NOT EXISTS {database}.{table_name} (
            exchange String,
            symbol String,
            bids Array(Tuple(Float64, Float64)),
            asks Array(Tuple(Float64, Float64)),
            timestamp Int64,
            local_timestamp Int64
        ) ENGINE = MergeTree()
        ORDER BY (local_timestamp, exchange)
        """
        
        self.execute_sql(sql)

    def create_trades_table(self, database: str, symbol: str = "btcusdt"):
        """
        创建 trade 表，用于存储交易数据
        
        Trade 表字段:
        - exchange: String
        - symbol: String  
        - timestamp: Int64
        - local_timestamp: Int64
        - id: UInt64
        - price: Float64
        - amount: Float64
        - side: String (buy/sell)
        """
        table_name = f"trade_{symbol}"
        
        sql = f"""
        CREATE TABLE IF NOT EXISTS {database}.{table_name} (
            exchange String,           
            symbol String,               
            timestamp Int64,      
            local_timestamp Int64,        
            id UInt64,                   
            price Float64,           
            amount Float64,          
            side String       
        ) ENGINE = MergeTree()
        ORDER BY (local_timestamp, exchange, id)
        """
        
        self.execute_sql(sql)

    def convert_parquet_to_depth_format(self, parquet_file: str) -> List[Dict[str, Any]]:
        """
        将 parquet 文件转换为 ClickHouse Depth 格式
        """
        table = pq.read_table(parquet_file)
        df = table.to_pandas()
        
        
        depth_records = []
        
        for _, row in df.iterrows():
            try:
                # 解析 JSON 字符串格式的数据
                if isinstance(row.iloc[0], str):
                    # 如果整行是一个 JSON 字符串
                    data = json.loads(row.iloc[0])
                else:
                    # 如果是列分离的格式
                    data = row.to_dict()
                
                # 转换为 Depth 格式
                depth_record = self._convert_to_depth_format(data)
                if depth_record:
                    depth_records.append(depth_record)
                    
            except Exception as e:
                print(f"处理行数据时出错: {e}")
                continue
        
        print(f"成功转换 {len(depth_records)} 条记录")
        return depth_records

    def _convert_to_depth_format(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """
        将单条记录转换为 Depth 格式
        """
        try:
            # 提取基本字段
            exchange = "okx"  # 固定为 okx
            symbol = data.get('symbol', 'BTC/USDT').replace('/', '_').upper()
            
            # 处理时间戳 - 确保是微秒级
            local_timestamp = int(data.get('local_timestamp', 0))
            if len(str(local_timestamp)) > 16:  # 纳秒级转微秒级
                local_timestamp = local_timestamp // 1000
            
            # exch_timestamp 通常是毫秒级，转换为微秒级
            exch_timestamp = int(data.get('exch_timestamp', 0)) * 1000
            
            # 处理 bids 和 asks
            bids = []
            asks = []
            bids_data = data.get('bids')
            if bids_data is not None and (
                (isinstance(bids_data, np.ndarray) and bids_data.size > 0) or
                (not isinstance(bids_data, np.ndarray) and bids_data)
            ):
                for bid in bids_data:
                    if isinstance(bid, dict):
                        price = float(bid.get('bid_price', 0))
                        amount = float(bid.get('bid_amount', 0))
                        bids.append((price, amount))
                    elif isinstance(bid, (list, tuple)) and len(bid) >= 2:
                        bids.append((float(bid[0]), float(bid[1])))
            
            asks_data = data.get('asks')
            if asks_data is not None and (
                (isinstance(asks_data, np.ndarray) and asks_data.size > 0) or
                (not isinstance(asks_data, np.ndarray) and asks_data)
            ):
                for ask in asks_data:
                    if isinstance(ask, dict):
                        price = float(ask.get('ask_price', 0))
                        amount = float(ask.get('ask_amount', 0))
                        asks.append((price, amount))
                    elif isinstance(ask, (list, tuple)) and len(ask) >= 2:
                        asks.append((float(ask[0]), float(ask[1])))
            
            return {
                'exchange': exchange,
                'symbol': symbol,
                'bids': bids,
                'asks': asks, 
                'timestamp': exch_timestamp,
                'local_timestamp': local_timestamp
            }
            
        except Exception as e:
            print(f"转换记录时出错: {e}, 数据: {data}")
            return None

    def insert_batch(self, database: str, symbol: str, records: List[Dict[str, Any]], batch_size: int = 1000):
        """
        批量插入数据到 ClickHouse
        """
        table_name = f"orderbook_{symbol.replace('_', '').lower()}"
        
        total_batches = (len(records) + batch_size - 1) // batch_size
        
        for i in range(0, len(records), batch_size):
            batch = records[i:i + batch_size]
            batch_num = i // batch_size + 1
            
            print(f"插入批次 {batch_num}/{total_batches} 到 {database}.{table_name} (共 {len(batch)} 条记录)")
            
            # 构建插入数据
            values = []
            for record in batch:
                # 转换 bids 和 asks 为 ClickHouse 格式
                bids_str = '[' + ','.join([f'({b[0]},{b[1]})' for b in record['bids']]) + ']'
                asks_str = '[' + ','.join([f'({a[0]},{a[1]})' for a in record['asks']]) + ']'
                
                value = f"('{database}', '{record['symbol']}', {bids_str}, {asks_str}, {record['timestamp']}, {record['local_timestamp']})"
                values.append(value)
            
            values_str = ',\n'.join(values)
            
            sql = f"""
            INSERT INTO {database}.{table_name} 
            (exchange, symbol, bids, asks, timestamp, local_timestamp)
            VALUES {values_str}
            """
            # print(sql)
            try:
                self.execute_sql(sql)
                print(f"批次 {batch_num} 插入成功")
            except Exception as e:
                print(f"批次 {batch_num} 插入失败: {e}")
                # 可以选择继续或停止
                continue

    def insert_to_both_databases(self, parquet_file: str):
        """
        将数据同时插入到 okx 和 okx_futures 数据库
        """
        print("开始插入数据到两个数据库...")
        
        # 转换数据格式
        records = self.convert_parquet_to_depth_format(parquet_file)
        
        if not records:
            print("没有可插入的数据")
            return
        
        symbol = records[0]['symbol'].replace('_', '').lower()
        
        # 插入到两个数据库
        databases = ["okx", "okx_futures"]
        
        for db in databases:
            print(f"\n正在处理数据库 {db}...")
            try:
                # 创建orderbook和trades表
                print(f"创建 {db} 数据库的表...")
                self.create_orderbook_table(db, symbol)
                print(f"orderbook表创建成功")
                self.create_trades_table(db, symbol)
                print(f"trades表创建成功")
                
                # 插入orderbook数据
                self.insert_batch(db, symbol, records)
                print(f"数据库 {db} 插入完成")
            except Exception as e:
                print(f"数据库 {db} 处理失败: {e}")


def main():
    """主函数"""
    print("开始 ClickHouse 数据插入任务...")
    
    # 初始化插入器
    inserter = ClickHouseInserter()
    
    try:
        # 创建数据库
        inserter.create_databases()
        
        # 插入数据
        parquet_file = "orderbook_btc_spot_okx_2024_10_07.parquet"
        inserter.insert_to_both_databases(parquet_file)
        
        print("\n数据插入任务完成!")
        
    except Exception as e:
        print(f"任务执行失败: {e}")
        import traceback
        traceback.print_exc()


if __name__ == "__main__":
    main()
