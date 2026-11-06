# HyperFold 查询行恢复：实现计划补充 v1

日期2026-11-06；Architect。来源：Jal1n `fd3f83db`、ProverDev `87b4f5da` / `2390e88a`。这是主计划v3.0的工程实验补充，不改变协议、参数证书、Transcript或证明格式。交ProverDev实现；Architect不改业务源码。

## 1. 范围与目标

实验参数固定M127二次扩域、μ=26、r=2、c=4、QA/RS两组、32线程、同一Setup/input/point。只测试第一棵中间树π_(d−1)的码字释放与查询恢复；顶层π_d完整保留，其他树仍按原路径。继续保留双向对角表，不混入释放正向表、求逆切换、磁盘卸载等变化。原142组正式数据不可覆盖。μ30试跑及全矩阵重跑不在这次实验范围。

本次数据衡量新策略的open净变化，不预承诺为正增幅或可忽略。理论减少的驻留码字是n_(d−1)·sizeof(E)：该μ26配置为4 GiB，μ30对应64 GiB；这不是全程RSS必降同样数值的保证。

## 2. 依赖入口：不修改Plonky3，不触碰私有字段

已核查当前pin下：
- `merkle-tree/src/merkle_tree.rs:31–66`中leaves/digest_layers私有，不能靠取出现有Tree来拆分。
- 同文件`new_over_leaf_digests`约140行：接受单个Matrix和已计算叶哈希，移动leaf_digests进入树。
- `builder.rs:357`开始的build_over_leaf_digests只在叶层以上建树；单矩阵条件下不需要枚举Matrix叶值。
- `mmcs/mod.rs:390`的open_batch_pruned通过Matrix::row读取查询行，然后从保留digest_layers走原生frontier；open_multi_batch复用此实现。

采用ProverDev提出的入口：从dense π_(d−1)平行计算一份LeafHash叶哈希；将可按需恢复行的Matrix描述符与叶哈希传给new_over_leaf_digests。树从构建开始就不拥有dense码字；dense独立保存到生成π_(d−2)后释放。**不是先建完整树再构建第二棵树**。

保持依赖pin、LeafHash/NodeHash及cap=0、二叉树不变。不通过Serde复制大树、不引入每行锁、不改共享vendor、不使用unsafe布局转换。new_over_leaf_digests不验证叶值与哈希相符，因此等价性测试属于本路径必检项。

## 3. 接口与所有权约定（由ProverDev按现有风格命名）

- 把第一树策略明确为互斥选择：原生保留、整树重建、预哈希保留控制组、预哈希查询恢复。现有open/default行为保持原生保留，旧重建入口兼容；不允许两个bool静默冲突。
- `RecoveredFirstMatrix<'a,F,E>`实现Plonky3 Matrix<E>：只借用顶层RowMajorMatrix<F>、对应顶层逆对角表及LocalMatrix<F>，持有固定长度beta和高度/宽度元数据。没有长为n_(d−1)的Vec，没有dense π_(d−1)的Arc或隐式持有者。
- 描述符的借用只存活于open调用，最终Proof无借用。顶层数据和Setup在整个open内稳定存活。
- Matrix必须实现实际行读取（例如row_unchecked返回栈数组的前r项迭代器），避免trait默认row/row_subseq互递归；safe row越界返回None。遵守trait unsafe签名契约，但实现内部不需要裸指针或unchecked indexing。
- `mmcs`开放辅助函数可泛化Matrix类型，或直接调用原生open_multi_batch；保持现有verify_multi_batch不变。不手写另一套multiproof剪枝。
- 第一棵lazy树单独保存，不硬塞进现有dense Tree<E>向量。dense first临时码字必须供下一次fold_level读取，而不是通过lazy Matrix把整条码字重新求一遍。
- d=1没有中间树：该策略明确定义为无操作；d=2存在第一树但无下一中间树：建好第一树后即可释放dense。d≥3：得到下一层矩阵后释放dense，尽可能在下一棵Merkle树构建前释放。

## 4. 恢复公式与索引不变式

n_i=c·k0·r^i。第一中间树高度h=n_(d−2)，宽r；请求其物理行j（0≤j<h）。对每个b∈[0,r)：

1. 顶层束索引 q=j+b·h。
2. 从顶层物理数组读取连续行 `top.values[q*r..(q+1)*r]`。
3. 使用顶层逆对角表 `diagonals.levels[d−1].inverse[q*(r−1)..(q+1)*(r−1)]`、顶层挑战beta_d和现有fold_row计算值。
4. 将值放入恢复行的第b个元素。

结果必须等于fold_level(...,i=d,beta_d)生成的dense第一中间矩阵第j行。不能把物理偏移j*r误当逻辑q，也不能使用d−2级对角表。

Phase2以既有顶层查询模h、排序去重后的索引调用open_multi_batch。恢复完整r元行后，再执行既有跨层单元省略，沿用确定性的最小父查询规则。不得只恢复r−1项或改变哈希顺序。每个唯一行最多r次局部fold，工作量O(r·Q_unique)个局部折叠；禁止扫描全码字。

## 5. 叶哈希构建与公平对照

new_over_leaf_digests可能改变第一树叶哈希的构建调度。为不把构造器差异误称为恢复开销，本次在每个底码下测三条路径：

A. NativeRetained：当前原生完整保留基线。
B. PrehashedRetained：用与C完全相同的并行LeafHash预计算和new_over_leaf_digests入口，但Matrix持有dense码字。
C. PrehashedRecovered：同入口，Matrix为恢复描述符，dense只作下一层折叠输入后释放。

B/C必须共享叶哈希预计算函数与粒度，叶哈希按自然物理行升序存放，仅算一次、move进树；不得额外clone叶哈希Vec。其Hash定义严格用现有LeafHash（字段字节数、规范字节及元素数帧均不变）。A的实现不为实验而改动。

- C−A：新方案相对正常运行的实际净影响，是用户关注的主指标。
- B−A：预哈希构造入口的净影响。
- C−B：相同构造入口下丢弃/恢复策略的净影响（包含内存局部性、分配生命周期变化，不是纯算术微基准）。

整树重建已有独立报告，本轮无须再跑；有余力才作为附加策略。

## 6. 正确性与资源验收

必须先测试再测μ26：
- 小规模双基域、QA/RS、r=2/4/8、c=2/4、d=1/2/3/4覆盖，恢复每行逐元素等于dense参考；查询包含首尾、奇偶、重复与跨层碰撞。
- A/B/C roots、原生PrunedMerklePaths的sibling次序与字节、完整Commitment/Proof字节一致；P/V后续挑战一致。测试比较实际字节，不只比较大小。
- 故意错误恢复一个值、调换行/坐标、篡改sibling，原生Verifier必须拒绝。
- 1/4/32线程一致性；覆盖超过并行分块阈值的叶哈希预计算。
- 测试专用计数器验证构树阶段不读lazy行、查询阶段只读查询行，不能为了验证而触发整表恢复。勿将逐行计数/时钟留在正式热路径。
- 检查dense第一码字在下一层矩阵生成后确实失去全部所有权；truncate不算释放。禁止每个查询复制完整表、clone哈希层、隐式缓存全码字。
- 单独记录进程峰值RSS并注明包括Setup/commit/所有open；不要把源码生命周期推断当作实测内存节省。

## 7. 计时协议与报告

同一优化release二进制，固定证书行、seed=42/27/28、同一个进程内复用一个commitment。A/B/C均维持双向Setup；不调用对角表释放或方向切换。

每底码使用三个独立进程轮次，策略顺序ABC、BCA、CAB；每策略进程1次预热+3次正式open，共2底码×3轮次×3策略×3=54条正式记录。这样每策略有3个进程均值可比较；报告重复结构，不把9条样本冒充9个独立进程。若资源安排需缩减，先明确标注先导测量而非稳定微小增幅结论。

记录：策略、底码、参数、进程轮次、重复号、open_ms、verify_ms、proof_bytes、fingerprint、process_peak_rss；保留原始command/status/manifest/二进制与源码hash。所有恢复、释放、哈希构建、multiproof提取都在open总计时内，不扣减后冒充基线。

细分计时优先使用第一中间树查询阶段总耗时（例如`first_tree_query_ms`，包含恢复行与路径提取），A/B/C在相同位置只计一次时钟。不要将它命名为纯row_recovery_ms。若另测纯恢复内核，单列诊断实验、说明与总open不能直接相减；避免为每行加计时或锁污染微小增量。

输出均值、进程内SD、三个进程均值及其波动、C−A/B−A/C−B的绝对差和比例，不能只报某次最快值。差异落入波动范围时写“本次未分辨出稳定增幅”，不宣称零开销。μ30正常路径无成功基线，不外推准确百分比。

## 8. 交接

本补充已定稿，可按Jal1n本次测量请求由ProverDev实现以上受限路径与实验；完成后提供提交与原始数据给Architect只读复审。若new_over_leaf_digests实际触发行枚举或需改共享依赖，停止该实现假设并反馈，不绕过接口封装。无需Theorist重算协议参数；若发现恢复式与现有Fold定义矛盾，再请求数学核验。
