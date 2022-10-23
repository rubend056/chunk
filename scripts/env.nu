#!/bin/nu

export def-env load_regex [] {
	open regex.toml | flatten | rotate --ccw | rename name value | reduce -f {} {|it,acc| $acc | upsert $"REGEX_($it.name)" $it.value} | load-env
}

def main [] {
	env_load regex.toml
}
