"""Operations the generated multi-tap reverb model (`tools/fx_model_rvbmulti.py`) calls.

The Leslie operations (`tools/fx_leslie_ops.py`) plus the packed-float and bit forms of the core.
"""

from __future__ import annotations

from fx_leslie_ops import *  # noqa: F403
from fx_leslie_ops import M32, lift, wd
from fx_pe import fdep_or, fdep_p, fext_p, fpack_p, funpack_p  # noqa: F401


def _btgl(v, bit):
    return (wd(v) ^ (1 << wd(bit))) & M32


#: VERIFIED (core `0x120D9D`, oracle trace): `Rn = BTGL Rx BY bit` flips one bit of the word.
ibtgl = lift(_btgl)
