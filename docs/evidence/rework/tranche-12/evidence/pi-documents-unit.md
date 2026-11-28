# Evidence — tranche-12 PI documents carry the unit

Both PI outputs are composed at project P0 with `unit='tranche-12'`, so `verify-project.py::_published_for_period` sees them for the tranche. The read-back is `documents.get` for each id and a fresh snapshot's `outputs.js` row carrying `unit: tranche-12`.
