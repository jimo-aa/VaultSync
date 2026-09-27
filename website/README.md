# VaultSync 官网（静态站点）

零依赖纯静态官网：双击 `index.html` 即可打开（刻意不用 ES Module，`file://` 下可用）。品牌沿用产品代际：暗色 `#10131A` + 香槟金 `#D9B25F`。

## 页面

| 页面 | 内容 |
| --- | --- |
| `index.html` | 产品介绍（六大能力 / 零信任安全模型 / Free-forever 十项红线 / 档位预览） |
| `pricing.html` | 完整定价：Free / Pro（买断 + 中继包）/ Team / Enterprise、能力对照表、到期语义承诺、明示不做清单 |
| `download.html` | Windows / macOS / Linux / 移动端下载（均为 Mock）、源码构建指引 |

## Mock 边界（重要）

- **购买**：`data-buy` 按钮打开居中 modal，提交后生成**未签名的演示许可证 JSON**（按 `docs/v2.0/12` 的 `VSL1` payload 字段表排版），可下载 `.demo.json`。不发起任何真实网络请求与扣款。
- **下载**：`data-download` 按钮仅弹 toast 提示（安装包尚未发布，首个正式版规划 v2.0.0）。
- **价格**：全部为示例数据。`docs/v2.0/12` §1.3 明确定价属商业决策、文档不给价目，故页面标注「示例价格，以正式发布为准」。

## 文案纪律（来自 `docs/v2.0/12` §三）

Free-forever 十项红线在所有页面均不得描述为「Pro 功能」；站点将其主动展示为产品立场（首页 `#promise` 区块 + 定价页对照表）。到期语义（§6.4）在 `pricing.html#terms` 明文呈现。

## 结构

```
website/
├── index.html / pricing.html / download.html
├── assets/style.css   # 全站样式（含居中 modal、toast、响应式）
└── assets/app.js      # Mock 商品目录 + 购买弹窗 + mock 许可证生成 + toast
```
