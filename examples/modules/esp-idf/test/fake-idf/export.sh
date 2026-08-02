# Stand-in for ESP-IDF's export.sh: puts the fake idf.py on PATH.
PATH="$(cd "$(dirname "$0")" && pwd)/bin:$PATH"; export PATH
