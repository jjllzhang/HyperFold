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
/* ids match Rust Kernel. All buffers are explicitly u64 limb arrays, not u128 casts. */
void fq_batch(unsigned op, size_t d, size_t n, const digit_t *aa,const digit_t *bb,
              digit_t *out,digit_t *scratch,int canonical) {
    if(op==15 || op==19){
        size_t block=op==15?1024:4096;
        if(n>block){for(size_t i=0;i<n;i+=block){size_t count=n-i<block?n-i:block;
            fq_batch(op,d,count,aa+i*d,bb+i*d,out+i*d,scratch+i*d,canonical);
        }return;}
    }
    digit_t *a=(digit_t*)aa,*b=(digit_t*)bb;
    digit_t x[4]={1,0,0,0},y[4]={0},z[4]={0},alpha[4]={17,0,19,0};
#define FIX(c) do {if(canonical)norm((c),d);}while(0)
#define q_add(a,b,c,d) do {q_add(a,b,c,d);if(canonical)norm((c),(d));}while(0)
#define q_sub(a,b,c,d) do {q_sub(a,b,c,d);if(canonical)norm((c),(d));}while(0)
#define q_mul(a,b,c,d) do {q_mul(a,b,c,d);if(canonical)norm((c),(d));}while(0)
#define q_sqr(a,c,d) do {q_sqr(a,c,d);if(canonical)norm((c),(d));}while(0)
#define q_inv(a,c,d) do {q_inv(a,c,d);if(canonical)norm((c),(d));}while(0)
#define q_mixed(a,b,c,d) do {q_mixed(a,b,c,d);if(canonical)norm((c),(d));}while(0)
#define EACH(expr) for(size_t i=0;i<n;i++){digit_t *u=a+i*d,*v=b+i*d,*c=out+i*d;expr;}
    switch(op) {
    case 0: EACH(q_add(u,v,c,d));break;
    case 1: EACH(q_sub(u,v,c,d));break;
    case 2: EACH(q_mul(u,v,c,d));break;
    case 3: EACH(q_sqr(u,c,d));break;
    case 4: EACH(q_inv(u,c,d));break;
    case 5: EACH(q_half(u,c,d);FIX(c));break;
    case 6: EACH(memcpy(c,u,d*8);fpneg1271(c);if(d==4)fpneg1271(c+2);FIX(c));break;
    case 7: EACH(fpmul1271(u,v,c);if(d==4)fpmul1271(u+2,v,c+2);FIX(c));break;
    case 8: /* Same 32 worker-dependent chains; no claim of single-thread latency. */
        for(size_t i=0;i<n;i++){q_mul(x,a+i*d,y,d);memcpy(x,y,d*8);}memcpy(out,x,d*8);break;
    case 9:
        memcpy(x,a,d*8);for(size_t i=0;i<n;i++){q_sqr(x,y,d);memcpy(x,y,d*8);}memcpy(out,x,d*8);break;
    case 10: /* WHT with input copy, no allocation. */
        memcpy(out,a,n*d*8);
        for(size_t base=0;base<n;base+=256){size_t count=n-base<256?n-base:256;
        for(size_t h=1;h<count;h*=2)for(size_t j=0;j<count;j+=2*h)for(size_t k=0;k<h;k++){
            digit_t *u=out+(base+j+k)*d,*v=u+h*d;
            q_add(u,v,x,d);q_sub(u,v,y,d);memcpy(u,x,d*8);memcpy(v,y,d*8);
        }}break;
    case 11: /* RS-like inner product. */
        for(size_t base=0;base<n;base+=256){size_t count=n-base<256?n-base:256;
        memset(x,0,sizeof x);for(size_t i=base;i<base+count;i++){q_mul(a+i*d,b+i*d,y,d);q_add(x,y,z,d);memcpy(x,z,d*8);}memcpy(out+base*d,x,d*8);}break;
    case 12: /* later E->E (or F->F) affine fold. */
        EACH(q_sub(v,u,x,d);q_mul(x,alpha,y,d);q_add(u,y,c,d));break;
    case 13: /* first F->E lift: ignore input imaginary coefficients. */
        EACH(fpsub1271(v,u,x);if(canonical)mod1271(x);fpmul1271(x,alpha,y);if(canonical)mod1271(y);fpadd1271(u,y,c);if(d==4)fpmul1271(x,alpha+2,c+2);FIX(c));break;
    case 14: /* Sumcheck three evaluations for adjacent pairs. */
        memset(out,0,3*d*8);
        for(size_t i=0;i+1<n;i+=2){
            digit_t *f=a+i*d,*w=b+i*d;
            q_mul(f,w,x,d);q_add(out,x,z,d);memcpy(out,z,d*8);
            q_mul(f+d,w+d,x,d);q_add(out+d,x,z,d);memcpy(out+d,z,d*8);
            q_add(f+d,f+d,x,d);q_sub(x,f,x,d);q_add(w+d,w+d,y,d);q_sub(y,w,y,d);
            q_mul(x,y,z,d);q_add(out+2*d,z,x,d);memcpy(out+2*d,x,d*8);
        }break;
    case 15: case 19: /* chunked prefix batch inverse, reusable scratch. */
        memcpy(scratch,x,d*8);
        for(size_t i=1;i<n;i++){q_mul(scratch+(i-1)*d,a+(i-1)*d,scratch+i*d,d);}
        q_mul(scratch+(n-1)*d,a+(n-1)*d,y,d);q_inv(y,x,d);
        for(size_t i=n;i-->0;){q_mul(x,scratch+i*d,out+i*d,d);q_mul(x,a+i*d,y,d);memcpy(x,y,d*8);}break;
    case 16: /* canonical roundtrip / normalization; limbs are the native encoding. */
        EACH(memcpy(c,u,d*8);norm(c,d));break;
    case 17: EACH(memcpy(c,u,d*8));break; /* control: copy + dispatch + FFI */
    case 18:
        for(size_t base=0;base<n;base+=256){size_t count=n-base<256?n-base:256;
        memset(x,0,sizeof x);for(size_t i=base;i<base+count;i++){q_mixed(a+i*d,b+i*d,y,d);q_add(x,y,z,d);memcpy(x,z,d*8);}memcpy(out+base*d,x,d*8);}break;
    }
#undef EACH
#undef FIX
#undef q_add
#undef q_sub
#undef q_mul
#undef q_sqr
#undef q_inv
#undef q_mixed
}
