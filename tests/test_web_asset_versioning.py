"""Production HTML revalidates while content-hashed assets are reusable."""
import asyncio
import re
from pathlib import Path

from src.web.app import serve_index


def test_index_revalidates_and_references_built_local_assets():
    response = asyncio.run(serve_index())
    assert response.status_code == 200
    assert response.headers["cache-control"] == "no-cache"
    body = response.body.decode()
    paths = re.findall(r'(?:src|href)="(/assets/[^\"]+)"', body)
    assert any(path.endswith(".js") for path in paths)
    assert any(path.endswith(".css") for path in paths)
    for path in paths:
        assert (Path("web") / path.lstrip("/")).is_file()
    assert "cdn.socket.io" not in body
    assert 'id="root"' in body