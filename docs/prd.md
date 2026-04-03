## 产品概述

**产品名称**：CipherSubmit Server

CipherSubmit Server 是一个面向课堂作业提交场景的服务端系统，负责接收学生提交、保存作业、执行链路模式审查、提供教师认证与取件接口，并通过 Vue 3 前端面板展示“平台能看到什么、看不到什么”。Vue 3 很适合这类管理后台和实体列表型页面，生态里也大量用于 admin dashboard 场景。 [madewithvuejs](https://madewithvuejs.com/admin-one)
本产品的核心价值不是做一个通用网盘，而是把“链路加密”和“端到端加密”在同一个作业提交流程里可视化呈现出来，让学生能直接观察服务端在不同模式下的数据可见性差异。

## 背景与目标

PRD 需要先明确背景、目标和成功标准，这样后续接口、页面和存储设计才不会漂移。 [atlassian](https://www.atlassian.com/agile/product-management/requirements)
本项目的教学背景是：课堂作业提交既需要隐私保护，也可能需要平台做一定审查，例如 ZIP 重复检测或 `.git` 痕迹检查，因此非常适合拿来对比不同加密模式下的中间节点能力差异。

**产品目标：**
- 为 `cisub` 提供稳定、固定的 HTTPS API。
- 支持两种提交模式：链路/审查模式、端到端模式。
- 支持教师挑战应答认证和受控取件。
- 提供一个清晰、适合投屏讲解的 Vue 3 可视化面板。
- 用尽量少的系统复杂度完成课堂演示闭环。

**非目标：**
- 不实现复杂 PKI、在线会话协商或匿名通信。
- 不隐藏客户端 IP、时间、大小等元数据；端到端加密并不自动隐藏这些信息。 [youtube](https://www.youtube.com/watch?v=ab17XI08pf4)
- 不兼容非 `cisub` 客户端。
- 不做通用对象存储平台。

## 用户与场景

**主要用户：**
- 学生：提交作业。
- 教师：认证、取件、解密。
- 授课者/演示者：通过前端页面讲解模式差异。

**核心场景：**
1. 学生以链路模式提交 ZIP，服务端可读取并检查其内容。  
2. 学生以端到端模式提交密文，服务端只能保存密文与元数据。  
3. 教师通过挑战应答流程证明自己持有私钥后取件。  
4. 授课者通过前端页面对比两种模式下服务端的“可见信息”。

## 产品范围

### In Scope
- HTTPS/TLS 服务。
- 学生提交接口。
- 教师挑战认证接口。
- 教师按学号/全量取件接口。
- 链路模式审查逻辑。
- 端到端模式密文保存逻辑。
- Vue 3 前端展示页面。
- 提交删除/保留策略。

### Out of Scope
- 系统级用户注册登录。
- 多教师、多课程复杂权限模型。
- 实时消息通知。
- 高级查重算法。
- 证书自动签发平台。

## 传输与信任模型

服务端所有接口都运行在 HTTPS over TLS 之上。TLS 握手过程中，服务端本来就会把自己的证书发送给客户端，因此客户端初始化时可以直接通过握手提取服务端叶子证书并保存指纹，而不需要服务端额外提供 `/cert` 下载接口。 [youtube](https://www.youtube.com/watch?v=ZkL10eoG1PY)
客户端后续不依赖系统 CA，而是依赖初始化时保存的服务端证书指纹进行校验；如果服务端证书变化，客户端必须重新初始化本地信任。 [docs.pingidentity](https://docs.pingidentity.com/pingdirectory/11.0/managing_servers_and_certificates/pd_ds_tls_handshakes.html)

## 功能需求

### 1. 提交接收

服务端必须支持两条独立提交路径，而不是单接口混合模式。  
这样可以保证 CLI 与服务端在字段、处理逻辑和教学语义上都足够清晰。

#### 1.1 链路模式提交
- 接收学生上传的明文 ZIP。
- 校验请求字段完整性。
- 校验 `file_sha256` 与服务端收到的 ZIP 原文是否一致。
- 保存原始 ZIP 与提交元数据。
- 触发链路模式审查流程。

#### 1.2 端到端模式提交
- 接收学生上传的 envelope 密文结构。
- 校验 envelope 字段完整性。
- 保存密文、加密后会话密钥、nonce 和元数据。
- 不尝试解密正文。
- 标记该提交为“正文不可见”。

### 2. 审查逻辑

仅链路模式启用审查。  
服务端需要完成以下能力：

- 解码并读取 ZIP。
- 检查 ZIP 内是否存在 `.git` 目录或显著相关痕迹。
- 计算并记录服务端侧文件 SHA-256。
- 对比历史提交，标记完全相同 ZIP 哈希的高疑似重复提交。
- 生成审查结果供前端展示。

### 3. 教师认证

教师认证采用挑战应答两步流程。  
私钥始终留在教师本地，服务端不能要求上传私钥，这是公私钥体系的基本边界。

#### 3.1 请求挑战
- 接收教师公钥。
- 生成随机挑战。
- 用该公钥加密挑战。
- 返回 `challenge_id` 和加密后的挑战。

#### 3.2 验证挑战
- 接收 `challenge_id`、挑战响应和教师公钥。
- 校验响应是否与原始挑战一致。
- 校验通过后发放短期 `access_token`。

#### 3.3 认证约束
- `challenge_id` 必须一次性使用。
- 挑战必须设置过期时间。
- token 必须短时有效。
- token 应与认证通过的公钥指纹绑定。

### 4. 教师取件

服务端必须支持两种取件方式：
- 按学号取件。
- 全量取件。

**统一要求：**
- 必须要求 Bearer Token。
- 返回结构统一为 `items` 数组。
- 每个条目都包含提交编号、学号、时间、模式、负载。
- `mode` 必须与 `payload.kind` 保持一致。
- 链路模式返回可直接落盘的原始文件内容。
- 端到端模式返回完整 envelope，由客户端本地解密。

### 5. 删除与保留策略

服务端必须定义明确的文件保留策略，不能把“是否删除”完全交给客户端决定。  
建议支持以下三种策略中的至少一种：

- 取件即删除。
- 取件后延迟删除。
- 手动清理。

默认建议采用“延迟删除”，因为这样更稳，能减少课堂演示时网络异常、误操作导致的数据丢失。

## 前端产品需求

Vue 3 很适合 admin dashboard 和实体列表型后台，尤其适合用来做表格、筛选、详情面板和状态展示。 [reddit](https://www.reddit.com/r/vuejs/comments/1q9vya8/qdadm_a_simpler_way_to_build_admin_dashboards_in/)
前端应尽量是单页应用，重点放在“信息清晰”和“模式差异可视化”，而不是堆很多后台管理功能。

### 页面要求

#### 页面 1：提交总览
展示：
- 提交编号
- 姓名
- 学号
- 文件名
- 提交时间
- 提交模式
- 文件哈希
- 状态标签

#### 页面 2：链路模式详情
展示：
- ZIP 哈希
- 是否命中 `.git`
- ZIP 内容摘要
- 是否与历史提交重复
- 重复对象列表
- 审查时间

#### 页面 3：端到端模式详情
展示：
- 文件名
- 学号
- 时间
- 哈希
- envelope 基本信息
- “正文不可见”状态
- “服务端不可解密”提示

#### 页面 4：教师认证与取件状态
展示：
- 最近挑战记录
- token 发放与过期时间
- 最近取件动作
- 删除/保留策略执行结果

### 前端交互原则
- 链路模式和端到端模式必须在颜色、标签和说明文案上明显区分。
- 前端必须一眼看出“服务端当前能看到什么”。
- 支持按学号和模式筛选。
- 页面适合课堂投屏，避免过多细碎控件。

## 数据需求

服务端至少需要维护以下逻辑实体：

### Submission
- `submission_id`
- `name`
- `studnum`
- `file_name`
- `file_sha256`
- `accepted_at`
- `mode`
- `payload_kind`
- `storage_path`
- `status`

### LinkInspection
- `submission_id`
- `has_git_dir`
- `zip_entries_summary`
- `duplicate_sha256`
- `duplicate_submission_ids`
- `inspected_at`

### TeacherChallenge
- `challenge_id`
- `public_key_fingerprint`
- `challenge_bytes`
- `created_at`
- `expires_at`
- `used`

### TeacherToken
- `token`
- `issued_at`
- `expires_at`
- `bound_public_key_fingerprint`

## 非功能需求

### 1. 协议稳定性
PRD 和接口文档必须共同作为单一事实源，接口路径、字段名、认证流程和 envelope 结构都应视为正式契约。 [mural](https://www.mural.co/templates/product-requirements-document)
后续任何改动都必须同步更新 CLI 和服务端。

### 2. 安全性
- 所有接口必须走 HTTPS。
- 服务端不得接收教师私钥。
- 挑战与 token 必须防重放。
- 证书变化必须导致客户端重新初始化信任。 [entro](https://entro.security/glossary/tls-handshake/)

### 3. 可维护性
- API 层、业务层、前端层职责分离。
- 前端页面围绕实体组织，减少重复页面胶水代码；实体中心式后台设计本身就更适合列表、表单、权限和筛选统一管理。 [reddit](https://www.reddit.com/r/vuejs/comments/1q9vya8/qdadm_a_simpler_way_to_build_admin_dashboards_in/)
- 日志和错误必须便于联调。

### 4. 可演示性
- 每个提交都必须明确显示模式。
- 演示者必须能快速切换查看链路模式与端到端模式的差异。
- 错误提示应尽量清楚，适合课堂展示。

## 约束与假设

- 当前客户端使用 HTTP/1.1。
- 所有请求和响应为 UTF-8 JSON。
- 二进制统一 Base64。
- 时间字段统一 RFC 3339。
- 教师认证依赖现有 challenge/verify 流程。
- 初始化信任依赖 TLS 握手读取服务端证书，而不是额外业务接口。 [auth0](https://auth0.com/blog/the-tls-handshake-explained/)

## 风险点

- 证书轮换会导致所有已初始化客户端失效，必须重新信任。  
- 如果链路模式的 ZIP 检测规则过于简单，可能出现误判或漏判。  
- 如果删除策略过激，可能影响课堂演示稳定性。  
- 如果前端没有把“正文不可见”和“元数据可见”区分开，教学价值会下降；端到端加密并不等于所有信息都隐藏。 [youtube](https://www.youtube.com/watch?v=aZRNz_Q9bwI)

## 验收标准

服务端达到可交付状态，至少需要满足：

- `cisub init <ip:port>` 能成功通过 TLS 握手获取证书并保存信任材料。 [youtube](https://www.youtube.com/watch?v=ZkL10eoG1PY)
- 链路模式提交后，前端能看到 `.git` 检测和重复哈希标记。
- 端到端模式提交后，前端只能看到元数据和不可解密状态。 [reddit](https://www.reddit.com/r/elementchat/comments/1f95jmp/the_filenames_including_the_filename_extension/)
- 教师能通过挑战应答拿到 token。
- 教师能按学号或全量取件。
- 删除/保留策略行为稳定可重复。
