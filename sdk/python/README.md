# SQLRite Python SDK

Python SDK for SQLRite HTTP query surfaces.

## Install (editable local)

```bash
pip install -e sdk/python
```

## Usage

```python
from sqlrite_sdk import SqlRiteClient

client = SqlRiteClient("http://127.0.0.1:8099")
print(client.openapi())
print(client.query(query_text="agent memory", top_k=2))
print(client.sql("SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"))
```
