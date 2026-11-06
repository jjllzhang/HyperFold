/* Benchmark glue; FourQ itself is compiled unmodified from the pinned checkout.
 * FourQlib Copyright (c) Microsoft Corporation, MIT license (see FOURQ-LICENSE).
 * x64 C path, no ASM/AVX macros. FFI crosses once per whole worker batch.
 */
#include <stddef.h>
#include <string.h>
#include "eccp2_core.c"

static void norm(digit_t *a, size_t d) { mod1271(a); if (d == 4) mod1271(a+2); }
static void q_add(digit_t *a,digit_t *b,digit_t *c,size_t d) {
    fpadd1271(a,b,c); if(d==4)fpadd1271(a+2,b+2,c+2);
}
static void q_sub(digit_t *a,digit_t *b,digit_t *c,size_t d) {
    fpsub1271(a,b,c); if(d==4)fpsub1271(a+2,b+2,c+2);
}
static void q_mul(digit_t *a,digit_t *b,digit_t *c,size_t d) {
    if(d==2)fpmul1271(a,b,c);else fp2mul1271((felm_t*)a,(felm_t*)b,(felm_t*)c);
}
static void q_sqr(digit_t *a,digit_t *c,size_t d) {
    if(d==2)fpsqr1271(a,c);else fp2sqr1271((felm_t*)a,(felm_t*)c);
}
static void q_inv(digit_t *a,digit_t *c,size_t d) {
    memcpy(c,a,d*8);if(d==2)fpinv1271(c);else fp2inv1271((felm_t*)c);
}
static void q_half(digit_t *a,digit_t *c,size_t d) {
    for(size_t j=0;j<d;j+=2){c[j]=(a[j]>>1)|(a[j+1]<<63);c[j+1]=(a[j+1]>>1)|((a[j]&1)<<62);}
}
static void q_mixed(digit_t *a,digit_t *b,digit_t *c,size_t d) {
    fpmul1271(a,b,c);if(d==4)fpmul1271(a+2,b,c+2);
}

#define DIM 2
#define CONTRACT 0
#define ENTRY fq_2_0
#include "fourq_kernel.h"
#undef ENTRY
#undef CONTRACT
#undef DIM

#define DIM 2
#define CONTRACT 1
#define ENTRY fq_2_1
#include "fourq_kernel.h"
#undef ENTRY
#undef CONTRACT
#undef DIM

#define DIM 4
#define CONTRACT 0
#define ENTRY fq_4_0
#include "fourq_kernel.h"
#undef ENTRY
#undef CONTRACT
#undef DIM

#define DIM 4
#define CONTRACT 1
#define ENTRY fq_4_1
#include "fourq_kernel.h"
#undef ENTRY
#undef CONTRACT
#undef DIM

void fq_batch(unsigned op,size_t d,size_t n,const digit_t *a,const digit_t *b,
              digit_t *out,digit_t *scratch,int canonical,const digit_t *alpha){
    if(d==2){if(canonical)fq_2_1(op,n,a,b,out,scratch,alpha);else fq_2_0(op,n,a,b,out,scratch,alpha);}
    else{if(canonical)fq_4_1(op,n,a,b,out,scratch,alpha);else fq_4_0(op,n,a,b,out,scratch,alpha);}
}
