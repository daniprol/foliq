# foliq retrieval check

What `foliq search` and `foliq context` return for each answerable question,
asked word for word, with no model. Made by `uv run python -m foliq_eval retrieval`.

| Group | Corpus | n | Parsed | File in top 10 | Page in top 10 | Answer in context | Context tokens |
| --- | --- | --- | --- | --- | --- | --- | --- |
| single_fact | library-10 | 8 | 100% | 100% | 100% | 100% | 1,883 |
| tables | library-10 | 8 | 88% | 100% | 100% | 62% | 1,885 |
| multi_hop | library-10 | 8 | 75% | 100% | 100% | 25% | 1,929 |
| needle | library-10 | 8 | 88% | 100% | 88% | 75% | 1,972 |
| needle | library-100 | 8 | 88% | 100% | 88% | 75% | 1,934 |
| needle | library-1000 | 8 | 88% | 100% | 88% | 75% | 1,959 |

## Evidence missing from foliq's text of the document

- `table-01`: '315.03'
- `hop-01`: '9.73'
- `hop-03`: 'learning rate is set as 0.001'
- `needle-05`: 'ν = 0.3'
