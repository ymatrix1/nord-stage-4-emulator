/* Runs on the CCES ADSP-21489 simulator; results stay in `out` and are saved from the Memory Browser. */
#define NOUT 4016
extern void probe(unsigned *out);
unsigned out[NOUT];
volatile unsigned done;
int main(void) { probe(out); done = 0xD0E5; return 0; }
