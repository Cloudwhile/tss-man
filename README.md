# TSS Manager

<p align="left">
  <a href="https://github.com/Cloudwhile/tss-man/actions/workflows/ci.yml"><img src="https://github.com/Cloudwhile/tss-man/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/Cloudwhile/tss-man/actions/workflows/ghcr-publish.yml"><img src="https://github.com/Cloudwhile/tss-man/actions/workflows/ghcr-publish.yml/badge.svg" alt="GHCR Publish" /></a>
  <a href="https://github.com/Cloudwhile/tss-man/pkgs/container/tss-man"><img src="https://img.shields.io/badge/GHCR-ghcr.io%2Fcloudwhile%2Ftss--man-2563eb?logo=docker&logoColor=white" alt="GHCR Image" /></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-MIT-0d9488" alt="MIT License" /></a>
</p>

TSS Manager 是面向 **TeamSpeak 3 / TeamSpeak 6** 服务器的 Web 远程运维管理平台。后端基于 **Rust (Axum + Tokio + SeaORM)** 开发，支持 **SQLite** 与 **PostgreSQL** 数据库；前端采用 **React 19 + TypeScript + Vite + Tailwind CSS v4 + Apache ECharts** 构建，编译后静态资源内嵌于后端可执行文件中，方便单机或容器化部署。

---

## 架构概览

```mermaid
flowchart LR
  Browser["Web 控制台<br/>React · Tailwind CSS · ECharts"]
  subgraph Server["tss-server (Rust / Axum)"]
    API["REST API · 认证与权限管理"]
    WS["WebSocket 实时状态流"]
    Hub["实例连接管理与定时调度"]
  end
  DB[("SQLite / PostgreSQL")]
  TS_TCP["TeamSpeak 3 / 6<br/>Raw TCP (10011)"]
  TS_SSH["TeamSpeak 3 / 6<br/>SSH Query (10022)"]
  TS_WEB["TeamSpeak 3 / 6<br/>WebQuery HTTP/S"]

  Browser <--> API
  Browser <--> WS
  API --> Hub
  API --> DB
  Hub --> DB
  Hub <--> TS_TCP
  Hub <--> TS_SSH
  Hub <--> TS_WEB
```

---

## 功能模块

- **多协议实例管理**：支持通过 Raw TCP (`10011`)、SSH (`10022`) 与 WebQuery HTTP/HTTPS (`10080/10443`) 接入多台 TeamSpeak 实例，管理下属虚拟服务器的启停、槽位上限与欢迎信息。
- **监控大盘**：基于 ECharts 展示 `1h / 24h / 7d / 30d` 在线语音客户端与活跃频道变化、平均延迟与丢包率趋势、上下行带宽速率及近期操作记录。
- **实时频道与用户工作台**：通过 WebSocket 同步频道树与在线客户端状态，支持创建/编辑/删除频道、发送全服广播，以及对在线用户执行移动频道、私信、Poke、踢出、封禁与服务器组调整。
- **权限组与特权密钥**：查看与编辑服务器组权限项、管理组成员，支持批量生成和复制一次性特权密钥（Privilege Keys）。
- **封禁、访客与日志审计**：集中管理封禁规则、检索历史连接用户、处理用户投诉，查看 TeamSpeak 服务端日志与平台操作审计日志。
- **虚拟服快照与定时备份**：支持创建加密快照、导出/导入快照文件、对比快照与当前实时服的频道差异、回滚恢复（自动保存回滚前备份），以及配置 Cron 定时备份策略。
- **自动化巡检与告警推送**：支持挂机超时自动移入指定频道、定时广播公告，以及离线或高丢包时的 Webhook 通知推送（可选 HMAC-SHA256 签名）。
- **账户安全与团队授权**：首次启动通过向导初始化管理员账户，支持启用 TOTP 二步验证、修改密码，以及按 `super_admin`、`server_admin`、`moderator`、`auditor` 角色分配实例管理权限。

---

## 快速部署

### Docker 一键启动（GHCR 镜像）

```bash
docker run -d \
  --name tss-manager \
  --restart unless-stopped \
  -p 3000:3000 \
  -v tss_data:/app/data \
  ghcr.io/cloudwhile/tss-man:latest
```

服务启动后访问 `http://127.0.0.1:3000`，按页面提示创建管理员账户即可进入控制台接入 TeamSpeak 服务器。

### Docker Compose 部署

```bash
# 默认使用 SQLite 存储
docker compose up -d

# 如需使用 PostgreSQL，修改 docker-compose.yml 中的 DATABASE_URL 后启用 postgres profile
docker compose --profile postgres up -d
```

### 源码构建

环境要求：Rust `1.85+`、Node.js `22+`、pnpm `10`（Linux 系统编译需安装 `pkg-config` 与 `libssl-dev`）。

```bash
git clone https://github.com/Cloudwhile/tss-man.git
cd tss-man

# 1. 构建前端资源
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build

# 2. 编译后端可执行文件
cargo build --release -p tss-server

# 3. 运行单元测试与协议集成测试（测试环境会自动启动内存模拟服务验证协议交互）
cargo test --workspace

# 4. 启动服务
./target/release/tss-server
```

---

## 环境变量

| 变量名 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `TSS_LISTEN_ADDR` | `0.0.0.0:3000` | HTTP 与 WebSocket 服务监听地址 |
| `TSS_DATA_DIR` | `./data`（容器内 `/app/data`） | SQLite 数据库文件与 `.master.key` 存放目录 |
| `DATABASE_URL` | `sqlite://<TSS_DATA_DIR>/tss-manager.db?mode=rwc` | 数据库连接地址，支持 `sqlite://...` 或 `postgres://user:pass@host:5432/db` |
| `TSS_MASTER_KEY` | 自动生成并保存至数据目录 | 32 字节加密主密钥（64 位 Hex 或 44 位 Base64），用于加密保存连接凭据与快照 |
| `RUST_LOG` | `info,tss_server=info,tss_tsproto=info` | 日志输出级别 |

---

## 目录说明

```text
tss-man/
├── .github/workflows/    # CI 检查与 GHCR 容器构建发布工作流
├── crates/
│   ├── tss-tsproto/      # TeamSpeak 协议客户端实现 (Raw TCP / SSH / WebQuery)
│   ├── tss-migration/    # SeaORM 数据库迁移脚本
│   └── tss-server/       # Web 服务、鉴权、状态同步与定时任务
├── frontend/             # React 前端源码
├── Dockerfile            # 容器镜像构建文件
└── docker-compose.yml    # Compose 编排配置
```

## 许可证

[MIT License](./LICENSE)
