# Delta Foundation 源码与许可治理（Source and Licensing Governance）

**版本：** v1.1
**状态：** 治理基线
**范围：** 公开的 Delta Foundation 源码、发行版、依赖与贡献。

本文是 Delta Foundation 源码许可与第三方依赖、捆绑资产、贡献许可及发布门禁的基线，说明各项许可义务及其执行方式。

## 1. Foundation 许可证

除非某个单独文件或捆绑的第三方资产另有声明，Delta 自有的 Foundation 源码以 Apache License 2.0 授权。

根目录的 `LICENSE`、`NOTICE`、`THIRD_PARTY_NOTICES.md` 与 `LICENSES/` 是具有权威效力的公开许可证文件。

## 2. 第三方依赖

依赖保留其上游许可证。新增依赖在发布前，必须审查其许可证兼容性与署名义务。

具有强 Copyleft（strong-copyleft）或源码可见（source-available）许可的依赖，若会对 Foundation 施加不兼容的分发义务，则须经过明确的法务审查（legal review），且不得默认添加。

## 3. 捆绑资产

捆绑的字体、图标、源码片段及其他资产，必须保留所需的版权与许可声明。当再分发需要时，完整的许可证文本存放于 `LICENSES/` 之下。

## 4. 贡献

贡献者以适用的 Foundation 许可证授权其被接受的贡献。贡献策略可能随项目发展而要求额外的来源证明（provenance）或 CLA / DCO 控制。

## 5. 可选扩展

公开的扩展合同并不决定独立分发的扩展的许可证。扩展自身的许可证与分发条款不属于 Foundation 源码许可的范围，前提是其遵守公开合同以及所有适用的第三方许可证。

## 6. 发布门禁

Foundation 发布 CI 校验：

- 根许可证的标识；
- 必需的声明（notices）；
- 已知的捆绑第三方署名；
- 清单（manifest）中的许可证声明；
- 依赖策略检查。

私有产品的许可、定价与分发策略，有意不在本公开的 Foundation 文件中定义。
