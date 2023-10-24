{% match struct.documentation() -%}
{% when Some with (docs) %}
    """
{% for line in docs.lines() %}    {{ line }}
{% endfor %}
    Attributes
    ----------
{% for f in struct.fields() -%}
{% match f.documentation() -%}
{% when Some with (docs) %}    {{ f.name() }} : 
        {{ docs|indent(8) }}
{% when None %}
{%- endmatch %}
{%- endfor %}    """
{% when None %}
{%- endmatch %}