public interface {{ interface_name }} {
    {% for meth in methods.iter() -%}
    {%- let func = meth -%}
    {%- include "FunctionDocsTemplate.kt" -%}
    {% if meth.is_async() -%}suspend {% endif -%}
    fun {{ meth.name()|fn_name }}({% call kt::arg_list_decl(meth) %})
    {%- match meth.return_type() -%}
    {%- when Some with (return_type) %}: {{ return_type|type_name -}}
    {%- else -%}
    {%- endmatch %}
    {% endfor %}
    companion object
}

