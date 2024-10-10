# 目录结构

```bash
zilean/
│
├── Cargo.toml               # 项目依赖与配置
├── README.md                # 项目说明文档
├── src/
│   ├── main.rs              # 主程序入口
│   ├── config.rs            # 配置文件处理模块
│   ├── core/                # 回测引擎核心逻辑模块
│   │   ├── mod.rs
│   │   ├── dataloader.rs    # 数据加载模块
│   │   ├── processor.rs     # 回测逻辑处理模块
│   │   ├── order.rs         # 订单管理模块
│   │   ├── market.rs        # 行情模块
│   │   ├── account.rs       # 账户管理模块
│   │   ├── stats.rs         # 基础统计功能模块
│   │   └── recorder.rs      # 结果记录模块
│   ├── ipc/                 # IPC 通信模块
│   │   ├── mod.rs
│   │   └── zmq_server.rs
│   └── utils/               # 工具模块
│       ├── mod.rs
│       ├── logger.rs
│       ├── display.rs       # 结果展示模块
│       └── time.rs
│
└── misc/
    └── config.toml          # 默认配置文件
```
