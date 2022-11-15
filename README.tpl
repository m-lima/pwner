# {{crate}}
[![Github]({{ repository }}/workflows/build/badge.svg)]({{ repository }}/actions?workflow=build)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Cargo](https://img.shields.io/crates/v/{{ crate | urlencode }}.svg)](https://crates.io/crates/{{ crate | urlencode }})
[![Documentation](https://docs.rs/{{ crate | urlencode }}/badge.svg)](https://docs.rs/{{ crate | urlencode }})

{{readme}}

{%- if links != "" %}
{{ links }}
{%- endif -%}
