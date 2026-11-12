/* Runs on the CCES ADSP-21489 simulator. The seed tables stay in memory (rec, rsq, in,
   flags_rec, flags_rsq) and are saved from the debugger's Memory view. No stdio. */
#define NEXP 22
#define NIN (NEXP * 256 + 12)
extern void probe(unsigned *rec, unsigned *rsq, unsigned *in);
unsigned rec[NIN], rsq[NIN], in[NIN];
volatile unsigned done;
int main(void) {
    probe(rec, rsq, in);
    done = 0xD0E5;
    return 0;
}
