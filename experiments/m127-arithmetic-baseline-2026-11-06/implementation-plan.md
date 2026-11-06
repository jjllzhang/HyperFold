# M127 同域性能对照、优化与路径统一：实施规范 v1.1

Architect，2026-11-06；请求Jal1n `c983b3c0`，协同ProverDev task #16，规划task #17。目标：测清当前M127与成熟实现的差距，按证据优化，删除被替代的内存分支，正式入口统一到已经在最大两组跑通的一次性释放路径。Architect只读查库与写规范，ProverDev实施。

## 0. 起点与执行顺序

此前μ30一次性策略两组成功已独立核验（消息50826bb3）：各3次正式测量、约447.94 GiB。旧142组、全部失败、μ26对照和新μ30成功结果仍是不同生命周期的数据集，禁止合成“原始统一模式144组已成功”。

当前前一轮所有实验已经结束（ProverDev消息938ac4e5）；新计时由ProverDev独立顺序运行，不与PCS大实验/编译/测试并发。用户已要求本轮比较、必要优化与清理，不需再次等待泛泛的“开始”确认。具体破坏性系统变更、swap/cgroup/kernel调整、merge/deploy不在范围。

提交与阶段分离：
1. 固定当前未优化M127与一次性流程基线，建立同域第三方正确性/性能harness。
2. 只测算术和小规模PCS相关核，报告真实差距；此时不改域实现或生命周期。
3. 按实测瓶颈独立优化M127；算术提交不混入旧路径删除。
4. 独立清理废弃内存路径、统一正式入口；证明/参数/编码不变。
5. 固定最终二进制，小规模验收；若采纳算术优化，按统一流程重跑M127全部72组。若没有值得采纳的优化，明确报告原因，仍完成用户要求的清理及回归，不伪造加速或无条件启动多日全量实验。

## 1. 参考实现已核实的身份与表示

### 1.1 自研当前基线

`src/field_m127.rs`：p=2^127−1，内部canonical u128，16B；4个64×64部分积宽乘后梅森折叠；inverse以p−2做二进制幂；未覆盖专用base square，二次扩域ext_mul是3次基域乘法的Karatsuba，ext_square当前沿用ext_mul(a,a)。这是源码观察，不是性能结论。

保留当前commit、Cargo.lock、native binary hash和输入种子，必要时在独立只读快照/benchmark比较包中构建旧实现；不要把旧业务内存模式永久保留在正式代码以维持基线。

### 1.2 arkworks：同域的通用Montgomery参照

本地`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ark-ff-0.6.0/`：
- `src/fields/models/fp/mod.rs:118`定义Fp128<P>=Fp<P,2>；用MontConfig派生同一p和经验证的generator（可用当前43），实例化Fp128<MontBackend<...,2>>。
- 这是**自行配置模数的ark-ff通用Montgomery实现**，不是声称arkworks提供了专用优化的M127命名域。
- 二次扩域配置相同X²+1，NONRESIDUE=−1，Frobenius系数[1,−1]，自然系数序(a,b)对应a+bi；专用nonresidue乘法用negation，别故意让通用乘法拖慢参考方。生成参数/接口均先通过正确性检查。
- `montgomery_backend.rs:541–547`的no-carry条件排除全1模数；M127恰落在排除条件。因此不能仅因打开asm feature/target-cpu=native就说该实例实际用了no-carry汇编。记录实际常量、编译feature和必要的反汇编证据，标明实际运行路径。
- 该文件约319行inverse为数据相关的二进制扩展欧几里得算法，不能将跨算法性能对比转述为侧信道性质相同。

### 1.3 FourQlib：专用梅森域参照，不引入生产依赖

本地参考仓库`~/zkp-research/reference/FourQlib`固定commit `1031567f23278e1135b35cc04e5d74c2ac88c029`（MIT，记录版本与维护状态，不当作仍积极维护的SNARK库）。
- `FourQ_64bit_and_portable/AMD64/fp_x64.h`：p=2^127−1专用乘法、平方、固定指数加法链inverse；`generic/fp.h`有portable实现。
- `eccp2_core.c`的fp2mul、fp2sqr、fp2inv使用同一a+bi / i²=−1，平方只用2次基域乘法；inverse使用norm和基域inverse。
- 专用inverse链是候选，不直接移植后宣称加速。需独立追踪每一步指数、最终p−2、运算计数和边界正确性；ProverDev源码审计报告给出的126次平方+12次乘法须由测试/指数检查锁定。
- 输出可能包含非规范零p，mod1271负责规范化；不能拿“允许冗余表示的裸运算”直接冒充我们canonical API的等成本替代。
- 初轮至少比较ark通用与FourQ portable/x64可用核心之一。若某汇编/平台分支无法构建，记录具体限制，不假称第三方不存在。对比编译C与Rust本地核，不测曲线标量乘法来代替域运算。

## 2. Harness边界与依赖管理

对比代码放独立benchmark-only包/工具目录（建议`benchmarks/m127-compare/`），锁定ark-ff=0.6.0、FourQ commit、编译器和features。不要把ark、C/FFI或新BigInt依赖带进PCS生产热路径；不全量升级根Cargo依赖或修改共享Plonky3 vendor。

FourQ wrapper只在benchmark侧使用，保留来源/许可证；编译已核查的必要文件，不运行上游不明安装脚本。FFI应在一次调用内完成整批循环，不能每次域加/乘跨一次语言边界。记下实际C/GCC/clang版本、flags、架构宏及是否ASM，所有输出采用安全明确定义的little-endian limb转换；不假设u128与C数组可直接随意转指针。

一次生成canonical输入集，再在计时外转成各库内部表示（ark Montgomery、FourQ limbs）。正确性输出统一成canonical字节在计时外比较；所有转换、规范化与序列化另设指标。

需分两种成本标签：
- **native_arithmetic**：各库原生可闭合表示下的算术内核，初始化/最终导出不计；FourQ允许的非规范值范围必须说明。
- **canonical_contract**：满足当前自研对外每次返回canonical元素的不变量时的成本；FourQ必要的mod1271包含在这里，不能漏算。纯转换/最终hash前批量规范化也单列。

Montgomery解码只在导出时进行，不在每个ark乘法后人为转回普通整数；FourQ裸运算与canonical契约结果分列，避免一种对照占便宜。最终是否更改生产内部表示另开设计权衡；首轮优先保留现有canonical invariant和序列化。

## 3. 正确性先行

统一域p、扩域多项式和系数顺序后，先运行以下非计时检查：
- base add/sub/neg/mul/square/inverse/halve，extension add/sub/mul/square/inverse、E×F；随机与边界值{0,1,2,p−1,p−2,2^63,2^64,2^126}及进位密集值。
- 至少10,000固定seed随机向量；独立BigUint模p参考验证宽乘/平方/边界。inverse仅非零参与性能比较，zero行为单独测试（Option/错误/约定不能混淆）。
- E inverse检查(a+bi)(a−bi)/(a²+b²)=1；i²=−1、Frobenius、基础field性质和已用生成元检查保持。
- canonical输入输出和拒绝≥p编码（16B base、32B ext）不变；FourQ冗余零归一化后再比较。
- 任何新增加法链逐步指数检查+随机field测试，任何延迟约简明确累加范围、溢出上界；不得把wrapping静默当作模p约简。
- 正确性失败则该backend数据不进入性能表，先修harness或实现。

## 4. 基准矩阵：32线程内的标量任务与批量工作负载

线程约束以Jal1n原始消息`be3f50f1`为准（已于本轮重新读取）：`--threads 1`只作功能支持，不实际运行benchmark。新需求未明确撤销该约束；本轮所有性能计时保持32线程。1线程只用于正确性测试，不将规范此前v1的单线程计时建议作为执行依据。

### 必测内核

| 层次 | 运算/任务 | 目的 |
|---|---|---|
| F单元素 | add/sub/mul/square/inverse/halve | 分清宽乘、专用square、求逆差异 |
| E单元素 | add/sub/mul/square/inverse、E×F | 对应Sumcheck/folding实际扩域路径 |
| 依赖链 | 每个工作线程内mul/square等连续依赖 | 32线程并发下的每线程链耗时，不能称独占单线程延迟 |
| 独立批次 | 多独立累加器/元素批量 | 测流水吞吐，防止只测一条依赖链 |
| 转换/编码 | canonical↔内部表示、规范化、16/32B输出 | 识别表示成本，不藏在边界 |
| 批量求逆 | 同一非零输入；当前P3 chunk1024及实际使用尺寸 | 单次inverse优势不等于Setup优势 |
| PCS相关核 | QA-WHT、M127 RS校验内积、首轮F→E折叠/后续E折叠、sumcheck round | 确认微优化会触及真实热点 |

内核包含L1/小块（如256/1024元素）、中等（2^14）、流式（2^20）三类工作集；报告bytes/element，避免全部变成内存带宽测试。批量输入、输出分配、PRNG在纯算术计时外；另提供真实含分配的核计时，标签不同。

- 统一release/native，所有计时使用32个工作线程；记录编译器差异。每个工作线程分别运行标量依赖链或独立元素任务，记录每线程操作数/耗时及整体wall/总吞吐，不将总操作数除wall所得ns/op当成单线程延迟。使用black_box/校验输出防优化消除，但不每操作都插重屏障导致只测黑盒开销。逆元连续两次会还原输入，勿以简化到无工作的小循环造假；给出非退化输入和循环定义。
- 批量比较保持同一32线程/分块任务/输入、同样非零要求，kernel内部循环摊薄FFI。记录实际任务安排：依赖链用32个独立工作者和启动屏障，避免Rayon仅创建线程池却只在调用线程执行一条链。32线程下的每线程耗时可能受调度/频率/共享cache影响，报告不得称独占核延迟。
- ark/FourQ支持的特殊乘法入口应公平使用（如E×F用专用混合乘，不嵌入E再做一般E×E）。
- 开始计时前热身；每backend/内核至少3个独立进程批次、每批至少5个计时窗口；交替/轮换顺序，不把同进程窗口当独立进程。选择足够迭代让窗口≥约100ms但不过度跑全矩阵；数量可随inverse调整并记录真实操作数。
- 固定seed与输入hash；顺序运行，无同时大PCS/编译任务。能在相同核心集合亲和绑定则各库一致绑定并记录；否则明确共享主机/NUMA/调频限制。不得为了微基准修改系统调频等共享服务。
- 输出ns/op或ops/s、原始窗口、进程均值/SD、backend/features/表示/操作数/线程/工作集/编译命令/hash；比值必须说明分母。归一化成本、FFI成本不混在某一库一侧。

## 5. 优化决策与验收（不先承诺差距）

把“差距明显”的工作阈值暂定为：相关算术或PCS核稳定慢约10%以上且大于进程间波动；这是触发调查的工程阈值，不是安全结论或用户必须接受的收益门槛。即使单inverse差距很大，也要看batch inverse/Setup与完整流程占比。

按证据优先考虑：
1. 固定指数inverse链替代通用binary pow；不改变zero返回和Field语义。
2. base专用square减少重复部分积；E专用square利用X²+1并覆盖Plonky3的ext_square入口。
3. 梅森约简/携带合并、branchless canonical加减等，以汇编和实测支持；不直接照抄依赖非canonical范围的FourQ算法。
4. 若相关内积确为热点，考虑有证明范围的延迟约简或小批量优化；SIMD/新表示/平台汇编属于复杂方案，须先说明内存布局与fallback，非首轮默认。

每项独立提交，保留before/after相同harness；默认目标是相关kernel稳定提升至少约5%且无主要PCS核稳定退化。若结果落在噪声内，保留更简单实现；如果优化主要减少Setup时间，报告Setup收益，不宣称commit/open同等加速。μ20/26一次性QA/RS代表组先比较完整Setup/commit/open/verify/释放/cleanup/wall与RSS；不以单微基准作为全72组加速依据。

必须通过：随机/边界差分、Field/extension属性、所有PCS四类测试及一次性所有权/错误路径测试；默认/native构建均验证；序列化、Commitment/Proof字节和Transcript挑战与冻结旧实现一致。编译器/表示变化导致字节不同时先调查，不直接更新golden视为通过。

当前实现不声称constant-time；若参考库某算法为数据相关，也要标明。性能结果不产生新的侧信道安全保证；不要以不安全的unchecked输入绕过canonical检查进入正常API。

## 6. 用户要求的旧路径清理与唯一正式入口

在上述基线冻结、正确性参照保存后，单独提交cleanup。清理目标是**正常运行时只有最新有效路径**，不是删除历史证据或全部朴素测试。

### 删除/退出正式业务路径

- `FirstTreeStrategy::Rebuild`、recompute_first_tree旧bool/OpenOptions、整棵第一树重新fold/commit分支、专用重建根比较代码。
- `setup_low_memory`、new_single、双向表原地切换prepare_commit/prepare_open及自动auto/low内存选择：正式one-shot已有预计算inverse后drop forward，不再往返求逆。
- 原生/预哈希完整保留A/B作为生产CLI可选模式、旧诊断bench入口、旧自动估算跳过/统一模式混合调度。必要的对照仅留在测试参考或历史版本。
- `resources.rs`若仅服务旧两种存储模式的估算，不能继续给新one-shot当上界。删除无调用旧估算/测试或改成明确的有效载荷诊断；不得保留误导的新模式安全门槛。
- `examples/recompute_overhead.rs`、旧query-row和old one-shot对照harness、对应调度脚本若只服务历史实验，移出活跃入口或删除当前树副本；历史commit和实验归档保留可重建版本。当前one-shot调度/分析复用的公共监控/contract工具先提取到中性模块，再删除旧文件，避免import断裂。

### 保留/统一

- 正式路径固定：标准Setup已有forward+inverse → commit → OpeningOnly转换drop forward → owned witness首轮释放 → 第一树哈希保留/按需行恢复 → verify/序列化 → caller oracle/参数cleanup。
- 对外commit仍提供建立承诺的必要能力；原可复用借用API如需保留供库用户使用，不再作为正式实验默认/自动fallback。若决定删除公开API，文档明确breaking research-API迁移；不可静默把借用函数变成消费函数。
- 保留朴素base/matrix/fold参考、原生MMCS一致性/篡改测试、byte/transcript golden和所有权compile_fail测试；不要把正确性oracle当“无用分支”删掉。
- `FirstTreeStrategy`最终是否可从业务接口消失由清理后的调用图决定；正式CLI不应再接受能静默改成重建/双向缓存的参数。旧flag应明确报移除/迁移错误，不能无声映射。
- 同一统一runner支持Goldilocks e3与M127 e2、QA/RS及当前全部r/c/μ；生命周期一致，参数表/证书/多项式编码/证明字节不改。本轮用户要求的完整性能重跑是M12772组；Goldilocks先做代表性回归，不自动再跑72个大规模组。
- 单独保留setup/input/commit/forward_release/open/verify/serialization/diagnostics/cleanup/wall与核心proof大小；所有正式重复fresh Setup。不同生命周期旧数据不混合resume，reuse按binary/schema/strategy/seed/参数/线程/重复/guard政策检查或新目录拒绝。
- 输出使用fresh路径保护/create_new，不允许覆写旧142组、各轮失败与成功记录。旧归档的纯数据分析脚本仍可独立运行；README指向历史commit/归档与新入口，不留失效示例。

### 清理验收

生成删除映射表：旧symbol/CLI/脚本→替代/历史位置→哪些测试继续保障。用文本调用搜索、cargo check/test/doc/Clippy及Python导入/轻量调度冒烟验证没有悬空引用。别在做表前大规模删文件。

对每基域×底码×r×c小规模遍历，统一one-shot应验证成功；原先最大两组的可运行性在最终二进制的完整M127重跑中复验。保留调用者Oracle的销毁时点，不能把cleanup挪入新open以假造“已统一”。

## 7. 最终M127实验与证据

如采纳算术优化：6个μ∈{20,22,24,26,28,30}×3个r×2个c×2底码=72组，e=2、原100-bit IOP参数，统一上述一次性路径，32线程，1预热+3正式重复，每组独立进程顺序运行。所有trial fresh Setup/输入/commit/open/verify，固定最终binary，不在半途中混新版本。最大两组也必须实际记录，不从旧成功结果复制计时。

先小规模与μ20代表组，后递增到μ30；运行状态/实际失败/partial样本有独立标记，完整3次成功才形成均值，失败汇总计时/size留空。内存观测与guard口径沿用当前用户指示，无估算替代实跑；不得自动改算法、whole-tree重建、swap或系统配置。

发布表分清：冻结未优化算术/新优化算术、旧cached生命周期/新one-shot生命周期。比较速度时同生命周期、相同规模参数与编译设置；不能用旧142行和新72行直接混合得出纯算术加速。至少μ20/26代表组提供同一one-shot的before/after；全72 before如未跑则明确不存在。

## 8. 完成标准

交付同域算术对照报告＋原始窗口/源码版本/规范化契约；优化前后正确性与实际性能证据；清理映射、唯一正式入口及迁移说明；在采纳优化时交付M12772组新实验及失败/成功真实记录。各阶段交Architect只读review，协议或有限域数学歧义交Theorist，不将测试称为安全证明。

当前本文仅基于源码及接口核查。未运行任何新算术或PCS性能测试，尚不知道自研与ark/FourQ的真实差距；不能在结果出来前宣称自研更慢或优化必有收益。
