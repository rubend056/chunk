#!/bin/nu

def do_kill [name,p] {
	echo $"($name) ($p | length) active"
	if ($p | length) > 0 {
		echo $"Killing ($name) process '(($p).name)'"
		$p | each {|p| 
			do -i {kill ($p | first).pid}
		}
		sleep 1sec
	}
}
export def stop [] {
	let p_name = "cargo";let p = (ps --long | find $p_name)
	do_kill $p_name $p

	let p_name = "yarn parcel";let p = (ps --long | find $p_name)
	do_kill $p_name $p

	let p_name = "chunk-app";let p = (ps | find $p_name)
	do_kill $p_name $p
}





