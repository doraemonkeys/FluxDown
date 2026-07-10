---
title: Docker 与 NAS
description: 用预编译 Docker 镜像运行 headless FluxDown 服务器，支持 Docker Compose、CasaOS/ZimaOS 与 Unraid。
section: headless-server
order: 2
sourceHash: "3f6c18b715a3"
---

运行 headless 服务器最快的方式是使用预编译 Docker 镜像——无需 Cargo 构建，也无需单独构建 Web 界面。镜像内置了服务器二进制和 Web 界面，全部通过一个端口（`17800`）暴露，并把数据库、日志和访问 token 持久化到卷。

镜像：`ghcr.io/zerx-lab/fluxdown-server`（标签：具体版本如 `0.1.54`，或 `latest`）。

> 为了部署可复现，建议钉具体版本标签而非 `latest`。

## docker run

```bash
docker run -d \
  --name fluxdown-server \
  --restart unless-stopped \
  -p 17800:17800 \
  -v fluxdown-data:/data \
  -v /path/to/downloads:/root/Downloads \
  ghcr.io/zerx-lab/fluxdown-server:latest
```

- `/data` 存放数据库、日志和生成的管理 token——请放在持久化卷上。
- `/root/Downloads` 是容器内的默认下载目录（`HOME=/root`）；绑定到你希望写入文件的宿主机路径。

管理 token 在首次启动时生成一次并打印到容器日志。抓取它：

```bash
docker logs fluxdown-server 2>&1 | grep -i token
```

用它登录 Web 界面，以及为管理 API 和 MCP 端点鉴权（`Authorization: Bearer <token>`）。

## Docker Compose

```yaml
services:
  fluxdown-server:
    image: ghcr.io/zerx-lab/fluxdown-server:latest
    container_name: fluxdown-server
    restart: unless-stopped
    ports:
      - "17800:17800"
    volumes:
      - fluxdown-data:/data
      - ./downloads:/root/Downloads
    # environment:
    #   FLUXDOWN_LANG: zh
    #   FLUXDOWN_DATABASE_URL: postgres://user:pass@host:5432/fluxdown

volumes:
  fluxdown-data:
```

```bash
docker compose up -d
docker compose logs fluxdown-server 2>&1 | grep -i token
```

[服务器部署](/docs/zh/headless-server/setup/)中的全部环境变量在此同样适用——最常用的是 `FLUXDOWN_LANG`（Web 界面默认语言，`en`/`zh`）和 `FLUXDOWN_DATABASE_URL`（指向外部 PostgreSQL 而非内置 SQLite）。

## CasaOS / ZimaOS

FluxDown 已发布为第三方 CasaOS / ZimaOS 应用商店，可一键安装。

在 CasaOS / ZimaOS 中：**应用商店 → 来源 → 添加**，填入：

```
https://cdn.jsdelivr.net/gh/zerx-lab/casaos-appstore@gh-pages
```

然后从商店安装 **FluxDown**。商店源：[zerx-lab/casaos-appstore](https://github.com/zerx-lab/casaos-appstore)。

## Unraid

Unraid Community Applications 模板见 [zerx-lab/unraid-templates](https://github.com/zerx-lab/unraid-templates)。Web 界面地址为 `http://[服务器IP]:17800/`。

## 安全地对外暴露

镜像在容器内绑定 `0.0.0.0:17800`，映射到宿主机。与任何 headless 部署一样，管理 token 是守护完整远程控制权的唯一屏障——在把它暴露到可信局域网之外前，请先阅读[反向代理与 TLS 指引](/docs/zh/headless-server/setup/)。

## 下一步

- [Web 界面](/docs/zh/headless-server/web-ui/)——在浏览器里登录并管理下载。
- [API 概览](/docs/zh/api/overview/)——用脚本或其他工具自动化服务器。
