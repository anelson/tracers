void {{provider_name_with_hash}}_{{probe.name}}(
    {%for arg in probe.args %}{{ arg.c_type }} {{ arg.name }}{% if !loop.last %}, {% endif %}{%endfor%}
) {
    STAP_PROBE{% if probe.args.len() > 0 %}{{ probe.args.len() }}{% endif %}(
	{{ provider_name_with_hash }},
	{{ probe.name }}
	{% for arg in probe.args %}{{ arg.c_type }} {{ arg.name }}{% if !loop.last %}, {% endif %}{%endfor%}
    );
}
