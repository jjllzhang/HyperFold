/* Thin scalar bridges into unmodified FourQ x64 C, MIT Microsoft.
 * Inputs are fixed arrays, disjoint output; no runtime operation/width dispatch.
 * No normalization here: closed [0,p] representation contract is audited separately.
 */
#include <stdint.h>
#include <string.h>
#include "eccp2_core.c"
#if !defined(UINT128_SUPPORT) || defined(ASM_SUPPORT) || (SIMD_SUPPORT != NO_SIMD_SUPPORT)
#error "comparison requires audited UINT128 C path without ASM/AVX macros"
#endif
_Static_assert(sizeof(digit_t)==8 && sizeof(felm_t)==16 && sizeof(f2elm_t)==32,"FourQ limb ABI");
void hf_fq_add(digit_t*a,digit_t*b,digit_t*c){fpadd1271(a,b,c);}
void hf_fq_sub(digit_t*a,digit_t*b,digit_t*c){fpsub1271(a,b,c);}
void hf_fq_mul(digit_t*a,digit_t*b,digit_t*c){fpmul1271(a,b,c);}
void hf_fq_square(digit_t*a,digit_t*c){fpsqr1271(a,c);}
void hf_fq_neg(digit_t*a,digit_t*c){memcpy(c,a,16);fpneg1271(c);}
void hf_fq_inverse(digit_t*a,digit_t*c){memcpy(c,a,16);fpinv1271(c);}
void hf_fq_halve(digit_t*a,digit_t*c){memcpy(c,a,16);fpdiv1271(c);}
void hf_fq_qmul(felm_t*a,felm_t*b,felm_t*c){fp2mul1271(a,b,c);}
void hf_fq_qsquare(felm_t*a,felm_t*c){fp2sqr1271(a,c);}
void hf_fq_qinverse(felm_t*a,felm_t*c){memcpy(c,a,32);fp2inv1271(c);}
void hf_fq_qmixed(felm_t*a,digit_t*b,felm_t*c){fpmul1271(a[0],b,c[0]);fpmul1271(a[1],b,c[1]);}
void hf_fq_qadd(felm_t*a,felm_t*b,felm_t*c){fp2add1271(a,b,c);}
void hf_fq_qsub(felm_t*a,felm_t*b,felm_t*c){fp2sub1271(a,b,c);}
void hf_fq_qneg(felm_t*a,felm_t*c){memcpy(c,a,32);fp2neg1271(c);}
void hf_fq_qhalve(felm_t*a,felm_t*c){memcpy(c,a,32);fp2div1271(c);}
