# CipherSubmit CLI HTTP 接口约定

## 1. 文档目的

本文档定义 `cisub` 客户端当前实现所依赖的 HTTP/TLS 协议约定，用于约束后续服务端实现，避免客户端与服务端在路径、字段名、认证流程和负载格式上发生漂移。

该文档描述的是**当前代码已经落地的契约**，不是抽象愿景。若后续协议变更，必须同步修改客户端代码与本文档。

## 2. 总体约束

### 2.1 传输层

- 所有接口都运行在 HTTPS over TLS 之上。
- 客户端在 `init` 阶段抓取服务端证书并保存 SHA-256 指纹。
- 后续请求不会走系统 CA 验证链，而是直接校验服务端证书指纹是否与本地保存值一致。
- 如果服务端证书变化，客户端会拒绝连接，直到重新执行 `cisub init <ip:port>`。

### 2.2 协议层

- 当前客户端使用 HTTP/1.1。
- 请求头固定包含：
  - `Host`
  - `Connection: close`
  - `Accept: application/json`
- 有请求体时额外发送：
  - `Content-Type: application/json`
  - `Content-Length`
- 教师取件接口需要 `Authorization: Bearer <token>`。

### 2.3 编码约束

- 所有请求体和响应体均为 UTF-8 JSON。
- 二进制文件内容统一使用 Base64 编码。
- 时间戳字段当前约定为 RFC 3339 字符串，例如 `2026-04-03T12:00:00Z`。

## 3. 初始化信任流程

初始化不依赖业务 HTTP API，而是直接通过 TLS 握手读取服务端证书。

客户端行为：

1. 连接 `cisub init <ip:port>` 提供的地址。
2. 完成 TLS 握手。
3. 提取服务端叶子证书。
4. 计算 `SHA256:XX:YY:...` 格式指纹。
5. 经用户确认后持久化：
   - `~/.cisub/server_cert.pem`
   - `~/.cisub/fingerprints/server.sha256`
   - `config.toml` 中的 `trusted_server_cert` 与 `trusted_server_fingerprint`

这一步不需要服务端额外实现 `/cert` 之类的接口。

## 4. 提交接口

客户端按模式拆分为两条提交路径，而不是单接口混用。

### 4.1 链路模式提交

`POST /api/v1/submissions/link`

用途：

- 学生提交明文 ZIP。
- 服务端可以直接读取 ZIP 内容并执行审查逻辑。

请求体：

```json
{
  "name": "Alice",
  "studnum": "20260001",
  "file_name": "homework.zip",
  "file_sha256": "4d4a...",
  "file_b64": "UEsDB..."
}
```

字段说明：

- `name`: 学生姓名。
- `studnum`: 学号。
- `file_name`: 原始文件名。
- `file_sha256`: ZIP 原文的 SHA-256 十六进制小写字符串。
- `file_b64`: ZIP 原文 Base64 编码。

成功响应：

```json
{
  "submission_id": "sub-1",
  "accepted_at": "2026-04-03T12:00:00Z",
  "server_message": "已接收明文 ZIP"
}
```

### 4.2 端到端模式提交

`POST /api/v1/submissions/e2e`

用途：

- 学生本地加密 ZIP 后提交。
- 服务端仅保存密文与元数据，不读取正文。

请求体：

```json
{
  "name": "Alice",
  "studnum": "20260001",
  "file_name": "homework.zip",
  "file_sha256": "4d4a...",
  "envelope": {
    "encrypted_key_b64": "...",
    "nonce_b64": "...",
    "ciphertext_b64": "..."
  }
}
```

字段说明：

- `name`、`studnum`、`file_name`、`file_sha256` 的含义与 link 模式相同。
- `envelope` 是客户端当前使用的加密信封结构：
  - `encrypted_key_b64`: 用教师 RSA 公钥加密后的随机 AES-256 会话密钥。
  - `nonce_b64`: AES-GCM 12 字节 nonce 的 Base64。
  - `ciphertext_b64`: ZIP 原文经 AES-256-GCM 加密后的密文 Base64。

成功响应：

```json
{
  "submission_id": "sub-2",
  "accepted_at": "2026-04-03T12:00:00Z",
  "server_message": "已接收密文文件"
}
```

## 5. 教师认证接口

教师认证采用两步挑战应答流程。私钥始终留在本地，服务端不能要求上传私钥。

### 5.1 请求挑战

`POST /api/v1/auth/teacher/challenge`

请求体：

```json
{
  "public_key_pem": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----\n"
}
```

成功响应：

```json
{
  "challenge_id": "challenge-1",
  "encrypted_challenge_b64": "..."
}
```

服务端要求：

- 生成随机挑战字节串。
- 使用教师公钥按 RSA-OAEP(SHA-256) 加密该挑战。
- 返回 `challenge_id` 和加密后的挑战。

### 5.2 提交挑战响应

`POST /api/v1/auth/teacher/verify`

请求体：

```json
{
  "challenge_id": "challenge-1",
  "challenge_response_b64": "...",
  "public_key_pem": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----\n"
}
```

字段说明：

- `challenge_response_b64` 是客户端使用本地教师私钥解密 `encrypted_challenge_b64` 后得到的原始挑战内容，再进行 Base64 编码后的结果。

成功响应：

```json
{
  "access_token": "teacher-token"
}
```

服务端要求：

- 根据 `challenge_id` 找回原始挑战。
- 校验 `challenge_response_b64` 解码后的字节串是否与原始挑战完全一致。
- 校验通过后发放短时有效的教师访问令牌。

## 6. 取件接口

### 6.1 按学号取件

`GET /api/v1/submissions/{studnum}`

请求头：

- `Authorization: Bearer <access_token>`

成功响应：

```json
{
  "items": [
    {
      "submission_id": "sub-2",
      "studnum": "20260001",
      "file_name": "homework.zip",
      "accepted_at": "2026-04-03T12:00:00Z",
      "mode": "e2e",
      "payload": {
        "kind": "e2e",
        "envelope": {
          "encrypted_key_b64": "...",
          "nonce_b64": "...",
          "ciphertext_b64": "..."
        }
      }
    }
  ]
}
```

### 6.2 取全部作业

`GET /api/v1/submissions`

请求头：

- `Authorization: Bearer <access_token>`

成功响应结构与按学号取件相同，只是 `items` 为全部可访问项。

## 7. Fetch 返回项结构

`items` 数组中的每一项都必须满足以下结构：

```json
{
  "submission_id": "sub-2",
  "studnum": "20260001",
  "file_name": "homework.zip",
  "accepted_at": "2026-04-03T12:00:00Z",
  "mode": "link | e2e",
  "payload": {
    "kind": "link | e2e",
    "...": "..."
  }
}
```

额外约束：

- `mode` 必须与 `payload.kind` 语义一致。
- `mode = "link"` 时：

```json
{
  "payload": {
    "kind": "link",
    "file_b64": "UEsDB..."
  }
}
```

- `mode = "e2e"` 时：

```json
{
  "payload": {
    "kind": "e2e",
    "envelope": {
      "encrypted_key_b64": "...",
      "nonce_b64": "...",
      "ciphertext_b64": "..."
    }
  }
}
```

客户端行为：

- `link` 负载直接 Base64 解码后落盘。
- `e2e` 负载由本地教师私钥解密后落盘。
- 文件统一保存到 `~/.cisub/cache/downloads/`。

## 8. 错误约定

当前客户端对非 `2xx` HTTP 状态码的处理非常简单：

- 只要不是 `2xx`，都会视为失败。
- 响应体会被按纯文本读取，作为错误消息展示给用户。

因此服务端至少应保证：

- 失败时返回正确的 HTTP 状态码。
- 响应体中包含可读错误信息。

推荐约定：

- `400 Bad Request`: 字段缺失、JSON 结构错误、模式和负载不匹配。
- `401 Unauthorized`: 缺少或无效 Bearer Token。
- `403 Forbidden`: 教师认证失败、权限不足。
- `404 Not Found`: 指定学号无作业，或接口路径不存在。
- `409 Conflict`: 重复挑战、重复提交等冲突场景。
- `500 Internal Server Error`: 服务端内部错误。

## 9. 当前未覆盖但建议明确的点

以下约束客户端目前依赖较弱，但服务端最好尽早固定：

- `submission_id` 的生成规则。
- `access_token` 的有效期和是否一次性使用。
- `fetch all` 的排序规则，建议按 `accepted_at` 升序或降序固定。
- 服务端是否允许同一学号多次提交，以及取件接口是否返回全部历史版本。
- `file_sha256` 在服务端是否校验并回存。

## 10. 变更规则

后续如果要改动以下任一项，必须视为协议变更：

- 接口路径
- JSON 字段名
- `payload.kind` 的取值
- 教师认证流程
- `Authorization` 头的使用方式
- e2e 信封结构字段

建议流程：

1. 先更新本文档。
2. 再同步修改客户端代码。
3. 最后调整服务端实现与联调测试。

否则非常容易出现“客户端能编译，但联调直接失败”的漂移问题。
