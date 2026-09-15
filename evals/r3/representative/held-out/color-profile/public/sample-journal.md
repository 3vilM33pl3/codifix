# Selected one-channel samples

| Asset | Stored sample | Profile | Provenance |
| --- | --- | --- | --- |
| a | 128 | Q | original source |
| b | 128 | P | v14 relabel of a, same raw-sample hash |
| c | 64 | P | reviewed Q-to-P conversion of a |
| d | 64 | unknown | imported without profile |

All samples are synthetic integers from 0 through 255. Under the stipulated
rounding, a's conversion yields c. A separate replay of the unconditional import
path turns c's 64 into 16 and keeps P. No display-device calibration, pixel
corruption, full color image or user preference study is supplied.
