# 安全策略

## 支持版本

| 版本 | 支持状态 |
|:---:|:---:|
| 1.0.x | 是 |
| < 1.0 | 否 |

## 漏洞报告

发现安全漏洞时，请通过以下方式报告：

- **GitHub Issues**: [提交安全漏洞](https://github.com/JularDepick/Maix-Agent/issues/new?template=security.md)
- **邮箱**: 通过 GitHub 私信联系项目作者 [JularDepick](https://github.com/JularDepick)

请勿在公开 Issue 中披露漏洞细节。

## 响应时间

- 确认收到报告：48 小时内
- 初步评估：7 个工作日内
- 修复发布：根据严重程度，14-30 个工作日内

## 安全最佳实践

- API Key 等敏感信息请通过 `.env` 文件配置，切勿提交到版本控制
- 使用 `shell` 类工具时，内置危险命令过滤机制（`rm -rf /` 等）
- 工具执行支持审批机制，可手动或自动批准
