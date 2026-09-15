# Reproducibility interpretation limits

Finite-precision addition in this fixture is order-sensitive: adding 1 to 1000
rounds back to 1000 at three significant digits, whereas cancellation first can
retain the 1. A seed for selection does not synchronize independent workers.
N14 explicitly chooses one reduction order rather than claiming all orders agree.

The two runs do not estimate a population variance, accuracy distribution or
model-quality gain. Matching output after a repair does not prove unbiased
scoring. Inputs, model, clock and other sources of nondeterminism still need
their own verification. Treat duplicate or missing results as invalid evidence,
not as extra samples or zeros.
