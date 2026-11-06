import csv,pathlib
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
p=pathlib.Path(__file__).resolve().parent
with (p/"strategy-means.csv").open() as f:rows=list(csv.DictReader(f))
variants=["ours","ark","fourq-native","fourq-canonical"]
labels=["Current canonical Rust","ark-ff Montgomery","FourQ x64 C native","FourQ normalize-every-op wrapper"]
colors=["#2563eb","#64748b","#f59e0b","#dc2626"]
fig,axes=plt.subplots(1,2,figsize=(12,4.8))
for ax,dim,kernels,names in [(axes[0],"1",["Mul","Square","BatchInverse","Wht"],["Multiply","Square","Batch inverse\n1024 items","WHT\n256 items"]),(axes[1],"2",["Mul","Square","Mixed","LiftFold"],["Multiply","Square","E x F","Lift F to E"])]:
 for j,(v,label,color) in enumerate(zip(variants,labels,colors)):
  group=[next(r for r in rows if r["backend"]==v and r["extension_degree"]==dim and r["elements"]=="32768" and r["kernel"]==k)for k in kernels]
  vals=[float(r["mean_worker_ns_per_item_mean"])for r in group]
  sd=[float(r["mean_worker_ns_per_item_process_sd"])for r in group]
  ax.bar([i+(j-1.5)*.2 for i in range(len(kernels))],vals,width=.19,yerr=sd,capsize=2,label=label,color=color)
 ax.set_xticks(range(len(kernels)),names)
 ax.set_title("Base field M127"if dim=="1"else"Quadratic extension, i²=-1")
 ax.set_ylabel("Worker kernel ns per input item")
 ax.grid(axis="y",alpha=.2);ax.set_axisbelow(True)
fig.suptitle("Same field, 32 resident workers, 1024 elements per worker")
handles,legend=axes[0].get_legend_handles_labels()
fig.legend(handles,legend,loc="lower center",ncol=2,frameon=False,bbox_to_anchor=(.5,-.01))
fig.text(.5,.105,"Error bars: sample SD of 3 independent process means. Not isolated single-thread latency.",ha="center",fontsize=9)
fig.tight_layout(rect=(0,.16,1,.95))
fig.savefig(p/"kernel-comparison.png",dpi=180,bbox_inches="tight")
fig.savefig(p/"kernel-comparison.svg",bbox_inches="tight")
