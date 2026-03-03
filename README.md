# syncai

> Peer-to-peer file sync tool for OpenClaw nodes.

**作者：小爆弹 💥** | [claw-works](https://github.com/claw-works)

---

## 是什么？

`syncai` 是一个轻量的点对点文件同步工具，专为 [OpenClaw](https://openclaw.ai) 节点间传输代码而设计。

典型场景：
- **A 机**（开发机/OpenClaw 主机）写代码
- **B 机**（测试机，按需启动）运行测试
- `syncai push` 将代码从 A 同步到 B，只传变更的文件

不依赖 SSH，不需要共享文件系统，两台机器在同一个 VPC 内就能直接用。

---

## 快速上手

### B 机（接收端）

```bash
# 启动 server，监听 9876 端口，接收文件到 /project 目录
SYNCAI_TOKEN=your-secret-token syncai server --port 9876 --dir /project
```

### A 机（发送端）

```bash
# 增量同步（只传变更的文件）
SYNCAI_TOKEN=your-secret-token syncai push ./my-project <B-IP>:9876

# 强制全量同步
SYNCAI_TOKEN=your-secret-token syncai push ./my-project <B-IP>:9876 --full

# 从 B 拉取文件到本地
SYNCAI_TOKEN=your-secret-token syncai pull <B-IP>:9876 ./my-project
```

---

## 工作原理

```
A（发送方）                          B（接收方）
                                    syncai server :9876
                                         |
syncai push ./project B-IP:9876          |
  1. 计算本地所有文件的 SHA256 hash       |
  2. POST /diff → 发送本地 manifest ──→ 比对差异
                                   ← 返回需要的文件列表
  3. 逐个上传差异文件 POST /file/* ──→ 写入磁盘
  4. 删除远端多余文件 DELETE /file/* ─→ 清理孤立文件
```

只传变更部分，大项目也很快。

---

## 安装

### 从 Release 下载（推荐）

```bash
# Linux arm64 (AWS Graviton)
curl -L https://github.com/claw-works/syncai/releases/latest/download/syncai-linux-arm64 -o syncai
chmod +x syncai

# Linux amd64
curl -L https://github.com/claw-works/syncai/releases/latest/download/syncai-linux-amd64 -o syncai
chmod +x syncai
```

### 从源码构建

```bash
git clone https://github.com/claw-works/syncai
cd syncai
cargo build --release
# 二进制在 target/release/syncai
```

---

## 配置

所有配置通过环境变量或命令行参数：

| 参数 | 环境变量 | 说明 | 默认值 |
|------|---------|------|--------|
| `--token` | `SYNCAI_TOKEN` | 认证 token（双端必须一致） | 必填 |
| `--port` | — | server 监听端口 | `9876` |
| `--dir` | — | server 接收目录 | `.` |

---

## 与 OpenClaw 集成

B 机启动时，在 user-data 里：

```bash
# 启动 syncai server
SYNCAI_TOKEN=$TOKEN syncai server --port 9876 --dir /project &

# 连回 OpenClaw 主机
OPENCLAW_GATEWAY_TOKEN=$GW_TOKEN openclaw node run --host <A-IP> --port 18789
```

A 机触发测试时：

```bash
# 1. 同步代码
syncai push ./project <B-IP>:9876

# 2. 在 B 上运行测试（通过 OpenClaw node exec）
```

---

## License

MIT

---

*Built with 💥 by 小爆弹 — part of the [claw-works](https://github.com/claw-works) ecosystem*
