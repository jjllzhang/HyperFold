/* Thin scalar bridges into unmodified FourQ x64 C, MIT Microsoft.
 * Inputs are fixed arrays, disjoint output; no runtime operation/width dispatch.
 * No normalization here: closed [0,p] representation contract is audited separately.
 */
#include <stdint.h>
#include <string.h>
#include "eccp2_core.c"
void hf_fq_add(digit_t*a,digit_t*b,digit_t*c){fpadd1271(a,b,c);}
void hf_fq_sub(digit_t*a,digit_t*b,digit_t*c){fpsub1271(a,b,c);}
void hf_fq_mul(digit_t*a,digit_t*b,digit_t*c){fpmul1271(a,b,c);}
void hf_fq_square(digit_t*a,digit_t*c){fpsqr1271(a,c);}
void hf_fq_neg(digit_t*a,digit_t*c){memcpy(c,a,16);fpneg1271(c);}
void hf_fq_inverse(digit_t*a,digit_t*c){memcpy(c,a,16);fpinv1271(c);}
void hf_fq_halve(digit_t*a,digit_t*c){digit_t half[2]={0,UINT64_C(0x4000000000000000)};fpmul1271(a,half,c);}
void hf_fq_qmul(felm_t*a,felm_t*b,felm_t*c){fp2mul1271(a,b,c);}
void hf_fq_qsquare(felm_t*a,felm_t*c){fp2sqr1271(a,c);}
void hf_fq_qinverse(felm_t*a,felm_t*c){memcpy(c,a,32);fp2inv1271(c);}
void hf_fq_qmixed(felm_t*a,digit_t*b,felm_t*c){fpmul1271(a[0],b,c[0]);fpmul1271(a[1],b,c[1]);}
