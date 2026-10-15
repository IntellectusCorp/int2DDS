"""IDL ``float128`` (``long double``) value.

Python has no binary128 float, so the value is carried as its raw 128 bits; going through
``float`` would drop the low 60 mantissa bits that peers compare. Conversions use ``Fraction``
so they are exact and correctly rounded without depending on the native library.
"""

from __future__ import annotations

import math
from fractions import Fraction

_MANT_BITS = 112
_EXP_MASK = 0x7FFF
_BIAS = 16383
_SIGN = 1 << 127
_MASK128 = (1 << 128) - 1


def _encode(negative: bool, x: Fraction) -> int:
    """binary128 bits of ``±x`` (``x >= 0``), ties to even."""
    sign = _SIGN if negative else 0
    if x == 0:
        return sign
    e = x.numerator.bit_length() - x.denominator.bit_length()
    if Fraction(2) ** e > x:
        e -= 1
    e = max(e, 1 - _BIAS)
    q = round(x / Fraction(2) ** (e - _MANT_BITS))
    if q == 1 << (_MANT_BITS + 1):
        q >>= 1
        e += 1
    if q < 1 << _MANT_BITS:
        return sign | q
    if e > _BIAS:
        return sign | (_EXP_MASK << _MANT_BITS)
    return sign | ((e + _BIAS) << _MANT_BITS) | (q - (1 << _MANT_BITS))


class Float128:
    """IEEE 754 binary128 bits. Equality is bitwise, like the native ``Float128``."""

    __slots__ = ("bits",)

    def __init__(self, bits: int = 0) -> None:
        self.bits = bits & _MASK128

    @classmethod
    def parse(cls, text: str) -> "Float128":
        """Decimal literal correctly rounded to binary128."""
        s = text.strip()
        negative = s.startswith("-")
        body = s[1:] if s[:1] in "+-" else s
        lower = body.lower()
        if lower in ("inf", "infinity"):
            return cls((_SIGN if negative else 0) | (_EXP_MASK << _MANT_BITS))
        if lower == "nan":
            return cls((_EXP_MASK << _MANT_BITS) | (1 << (_MANT_BITS - 1)))
        if not body or "/" in body or "_" in body:
            raise ValueError(f"invalid float128 literal {text!r}")
        return cls(_encode(negative, Fraction(body)))

    @classmethod
    def from_float(cls, value: float) -> "Float128":
        """Exact widening of a double."""
        if math.isnan(value):
            return cls((_EXP_MASK << _MANT_BITS) | (1 << (_MANT_BITS - 1)))
        negative = math.copysign(1.0, value) < 0
        if math.isinf(value):
            return cls((_SIGN if negative else 0) | (_EXP_MASK << _MANT_BITS))
        return cls(_encode(negative, Fraction(abs(value))))

    def __float__(self) -> float:
        """Nearest double."""
        negative = bool(self.bits & _SIGN)
        exp = (self.bits >> _MANT_BITS) & _EXP_MASK
        mant = self.bits & ((1 << _MANT_BITS) - 1)
        if exp == _EXP_MASK:
            return math.nan if mant else (-math.inf if negative else math.inf)
        if exp == 0:
            x = Fraction(mant) * Fraction(2) ** (1 - _BIAS - _MANT_BITS)
        else:
            x = Fraction(mant | (1 << _MANT_BITS)) * Fraction(2) ** (exp - _BIAS - _MANT_BITS)
        try:
            f = float(x)
        except OverflowError:
            f = math.inf
        return -f if negative else f

    def to_bytes(self, little_endian: bool = True) -> bytes:
        return self.bits.to_bytes(16, "little" if little_endian else "big")

    @classmethod
    def from_bytes(cls, data: bytes, little_endian: bool = True) -> "Float128":
        return cls(int.from_bytes(bytes(data[:16]), "little" if little_endian else "big"))

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Float128) and self.bits == other.bits

    def __hash__(self) -> int:
        return hash(self.bits)

    def __repr__(self) -> str:
        return f"Float128({float(self)!r}, bits=0x{self.bits:032x})"
