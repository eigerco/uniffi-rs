class {{ protocol_name }}(typing.Protocol):
    {%- call py::docstring_value(protocol_docstring, 4) %}
    {%- for meth in methods.iter() %}
    def {{ meth.name()|fn_name }}(self, {% call py::arg_list_decl(meth) %}):
        {%- let func = meth -%}
        {% include "MethodDocsTemplate.py" %}
        raise NotImplementedError
    {%- else %}
    pass
    {%- endfor %}
