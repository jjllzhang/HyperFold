# HyperFold 按最后使用点释放内存：一次性生命周期方案 v1

2026-11-06，Architect；响应Jal1n `fccd3d23`，task #13。本文是工程规划，不修改业务源码。目标：在已经实现的查询行恢复基础上，优先释放真正不再使用的数据，不用大规模重算换内存。实施前需明确采用一次性API及实验新口径；不能把现有重复commit/open接口静默改成一次性。

## 1. 已实际查阅的依据

- 用户附件`main.pdf`，SHA256 `4eab0fa4b9031959baba394608eb234a3604694469cdb7748e0741f0efb1db4c`。本轮重新提取并阅读§4（pp.23–26）、Protocol4.1/4.2；已渲染核对p.26的Phase1/Phase2顺序。
- `hyperfoldable_codes.md` §3.2–3.4编码、§4.1局部Fold：正向HᵀD用于编码，D⁻¹(Hᵀ)⁻¹用于折叠。
- 代码：`src/pcs.rs`约152–180（commit）、244–345（sumcheck/fold/first_dense释放）、后续Phase2及verify；`src/params.rs`对角表/prepare_commit/open；`src/sumcheck.rs`表构造与原地折叠；`src/encoder.rs`恢复Matrix；`src/bin/bench.rs`重复生命周期。

Protocol4.2在第一Sumcheck子轮后，后续子轮只依赖已经折叠的f；Phase2依赖f^(0)、码字查询与折叠挑战，不重新使用原始消息表。码字oracle必须仍可被查询，不等于所有原始工作数组都要同时保留。工程等价性仍须测试，不是对完整安全性的证明；两个重点最后使用点已获Theorist协议侧核对（消息e9eb87af，2026-11-06 08:43Z），条件见§8。

## 2. 推荐优先组合：已有恢复模式＋两个无重算释放点

### A. 正向对角表：最后一次commit返回后释放（最大组约64 GiB）

当前Setup同时保留forward与inverse；commit编码用forward，open/verify实际用inverse。commit结束且不再提交新多项式后，forward不再读取。保留已有inverse、base code、H及原parameters_id即可继续open/verify；不重采样、不重新求逆、不改变Transcript。

必须是显式一次性转移：建议新增`OpeningOnlyParams`一类只读状态，从原参数对象消费式转换；只暴露open/verify，不暴露commit或prepare_commit。若实现选择运行时状态检查，也须明确拒绝commit/prepare_commit，而不是沿用当前prepare_commit把inverse搬回forward再求逆。转换需要真正drop/mem::take forward allocation，clear/truncate不足。

这限制参数缓存的生命周期，不是数学上禁止原承诺多点打开：如果witness仍可用，可在只读inverse参数下打开。但本组合B会同时消耗原消息，因此本次整体API仍是单次打开。

### B. 原始基域消息f^(d)：第一个Sumcheck子轮的f′构造完成即释放（最大组16 GiB）

`pcs.rs`的first分支先用data.message计算g，然后采alpha，用同一message生成扩域f′。在f′完成后，原message的最后一次读取已经结束，之后用f′、后续f及顶层码字进行工作。

释放点应在第一次`fold_table(message,...)`完成后、随后weights折叠/缩容及第一中间树构建前。对r=4/8同样是第一个子轮结束，不必等待整个ν轮。

现有`open(&CommitData)`借用接口支持重复打开，不能擅自释放调用者的message。新增**消耗witness、借用committed oracle**的API：
- `CommitData`可消费式拆成`OwnedWitness`和`CommittedOracle`（名字由实现者按项目风格确定），都只move，不clone大Vec。
- Oracle拥有顶层tree、root与所需绑定元数据；witness拥有message。
- 一次性open消费OwnedWitness，但借用Oracle；原借用open保持兼容。
- 在共同内部实现中区分borrowed/owned witness，原first分支完成后释放owned一支；不要复制一整套协议循环导致两路径漂移。
- 顶层Oracle由调用者持有到verify/序列化后再销毁，和旧baseline一致。避免因“消费整个CommitData”在新open返回前额外销毁整个顶层树，把大树析构不公平地混进新open时间。

### 节省口径

上述A+B相对**已经启用第一树查询行恢复**再减少约80 GiB后续主要驻留量（64+16）；第一树64 GiB收益已在现有模式中，不重复计入。相对最初全保留模式，合计主体减约144 GiB，但不同阶段的峰值不能简单整体相减。

## 3. 最后使用点表：能释放与不能提前释放

| 对象 | 最后使用点 / 约束 | 动作 | 新增收益/成本 |
|---|---|---|---|
| 正向对角表 | 最后一次commit编码完成；y0计算不使用它 | commit返回后单向转移时drop | 最大约64 GiB；仅析构/释放成本，无额外域运算 |
| 原始消息表 | 第一Sumcheck子轮g和f′均已完成 | 仅owned witness路径drop | 16 GiB；释放计入open |
| 第一中间dense码字 | 下一中间码字已完成，lazy树只借顶层 | 现有first_dense=None | 已实现64 GiB，不计新收益 |
| f与W旧活动区 | 当轮折叠完成 | 现有原地折叠＋每层shrink_to_fit | 已实现；缩容可能搬迁，不能再声称新省全部W |
| 原始eq(z0)/eq(z)完整表 | 不需要同时物化；构造W用小因子 | 已有combined_weights | 已实现，不重复计益 |
| W最终表 | Phase1结束/终表吸收后 | 现有drop(weights) | 剩余很小，不解决大峰值 |
| 顶层码字与哈希 | Phase2顶层opening及lazy第一树恢复均完成 | 此前必须保留；之后可按原API销毁 | 不能Phase1提前丢；顶层重编码不是本方案 |
| 其他中间树码字和hash | query抽样后提取该树全部opening | 当前into_iter逐树消费 | 已较及时；在Phase1不能直接丢而不提供恢复机制 |
| 所有inverse表 | Phase1折叠、Phase2恢复/局部校验和verify可能仍用 | 本方案完整保留 | 不可仅因某层dense fold结束就无条件drop |
| 终表f0、roots、sumcheck消息 | 证明序列化/验证使用 | 保留 | 相比大数组很小，删除无收益且破坏证明 |

特别注意：查询依赖所有根、sumcheck消息和终表后的Transcript。不能提前抽query以便预留少量行；这改变协议顺序。若要在Phase2先收集所需逆对角元并释放完整表，需要新的稀疏验证参数接口，且不能降低Phase1主峰值，暂不优先。

## 4. 最大配置分阶段静态载荷

仅M127 μ30 r2 c4，n0=256,d=24。数字是数组有效载荷近似GiB，不含allocator/page table/额外capacity/OS或其他进程，不是RSS上界。

| 时点 | 现有恢复＋双向表 | 拟议恢复＋只读inverse＋释放message |
|---|---:|---:|
| commit结束，尚未释放forward | 336 | 336（相同） |
| forward释放后，open前 | 336 | 272 |
| 第一f′构造完成，message尚未释放、W尚未缩容 | 384左右 | 320左右 |
| 第一中间树完成，f/W已缩容 | 496 | 416 |
| 下一矩阵生成、first_dense尚未释放 | 512 | 432 |
| 第二中间树完成 | 480 | 400 |
| 第三中间树完成 | 504 | 424 |
| 第四中间树完成 | 516 | 436 |
| Phase1末尾，查询尚未逐树释放 | 528 | 448 |

原始消息16 GiB只在首个f′完成后消失，不能从此前每个阶段统一扣掉。A的64 GiB只在commit完成后消失，也不能从Setup/commit峰值扣掉。初始sumcheck缩容可能同时存在旧W32 GiB、新W16 GiB、f′16 GiB；消息已释放后主体约256+64=320 GiB，需在实际实现中再次确认该分配顺序。

这使主体载荷低于主机约503 GiB，提供更合理的尝试空间，但不能保证全程跑通。应使用本地digest_storage/实际Vec capacity和阶段观测复核，而不是宣称“峰值严格448”。两次历史失败日志没有阶段事件，因此不能据其HWM确认某个释放点已执行。

## 5. 公平计时与重复实验：不能隐藏重新Setup

一次性参数转移后没有forward，下一轮不能复用旧pcs做commit。建议每个独立测量试验从同一seed重新Setup，**参考组也按相同外层生命周期**：

Setup → 输入和声明值准备 → commit → forward-release转换 → 单次open（第一子轮内释放message）→ verify → 序列化 → Oracle/参数清理。

- 记录`setup_ms`、`commit_ms`、`forward_release_ms`、`open_ms`、`verify_ms`、`serialization_ms`、`cleanup_ms`及外层总wall时间。总wall必须说明是否包含输入生成与声明值求值，并保持对照一致。
- commit算法不变；open含message释放、现有dense释放、查询恢复及路径提取，不能从open扣除这些耗时。
- forward释放在commit/open之间单列，同时至少给出`commit+release+open+verify`作为明确包含转换的流程成本。更完整的wall可包含Setup/输入/清理，不能只挑不含Setup的数字说“一点重算都没有”。
- **单次commit→open内部不新增域重算**；独立重复必须重新Setup的成本另计。不能调用prepare_commit再把这段重算藏在外部未报告区域。
- 模式字段明确：`first_tree_strategy=prehashed_recovered`、`parameter_lifecycle=opening_only`、`witness_lifecycle=consumed_after_first_round`、`setup_per_trial=true`。
- 与μ26已完成A/B/C研究不是同一生命周期，必须单独归档，不改142组、μ26研究或旧最大组失败记录。

## 6. 实现步骤与验收建议（待选择实施）

1. 增加一次性参数状态及owned witness入口，保持原借用接口和默认行为不变；禁止prepare_commit恢复，不加业务热循环锁、不clone大表。
2. 小规模双基域/底码/r/c/d组合比较旧路径与新组合的commit/proof实际编码字节、P/V后续挑战；verify应在只读inverse状态下正常工作。
3. 单独测试forward allocation真正释放（长度/capacity/所有者）、owned message在第一fold后释放；不可只调用clear。验证旧API仍能多次open，一次性参数不能再次commit。
4. 校验失败时返回Err且释放所有owned资源；不能为了保持某条失败后可重试语义而暗中保留原witness。
5. μ26先比较相同外层一次性生命周期下“仅第一树恢复”和“恢复+A+B”，固定seed，独立进程交替；记录阶段计时/整体RSS，估算预期新增主体省(64+16)/16=5 GiB，但不承诺精确RSS差。
6. 若选择继续μ30实跑，先审查提交/小规模结果；仍顺序单进程，无自动策略切换，成功才写完整均值，否则留空并保留真实记录。内存guard政策须重新沿用当轮最新人类指示，不擅自恢复旧限制。
7. 每次独立试验须完整清理前次Oracle/参数再建立下一次Setup；否则跨轮重叠会抵消节省。

## 7. 次优选项与暂不推荐

- 若A+B仍不够，扩展仅查询行恢复到第二棵中间树可再减约32 GiB；但需递归恢复第一层，增加查询算术和接口复杂度。不能把任意多层递归当作低成本：深度增加时工作量可能按r的幂增长。
- 利用Wγ的两项张量结构避免完整W向量，可再降低早期buffer占用，但需重新设计sumcheck权重访问、测量额外运算并由Theorist核对；当前瓶颈不优先于直接释放。
- 不优先整树重建（μ24/26已测增加约31%–39%）、磁盘换页/外存树（I/O改变计时）、重算对角块（额外采样/求逆）；不把压缩hash/减少安全参数当作内存优化。

本文给出推荐组合与限制，未在本轮修改业务源码或验证新组合峰值。协议最后使用点已获条件性核对（见§8）；实验是否能完成只由实际结果确认。

## 8. 协作核对状态

ProverDev已提供只读源码审计：`~/zkp-research/notes/hyperfold-memory-last-use-code-audit-2026-11-06.md`（消息cb8a6f7d），其最后读取点、原消息释放时点、参数恢复风险和静态载荷与本文一致。Theorist在task #14独立核对协议前提，已于消息e9eb87af确认两项释放在当前调用链成立：
- forward释放后不再commit，且中间码字始终通过既有码字折叠生成，不能改用从折叠消息重新编码的路径；底码参数继续保留。
- y0与声明值y已确定；第一子轮消息与挑战处理完、完整扩域f′生成后，才释放原message。r=4/8不必等整层结束。
- 顶层码字、inverse与折叠参数依照后续查询用途保留，不改变挑战顺序或Transcript。

该核对不是对实现、RSS或最终运行成功的保证；约80 GiB仅指可移除数组载荷。新一次性接口仍需独立测试与代码审查，本文不把此协议核对当作人类新一轮实现/实验指令。
