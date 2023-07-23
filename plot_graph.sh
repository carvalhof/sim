#!/bin/bash

LAYOUT=$1

if [[ -z $LAYOUT ]]; then
	exit 1
fi

./script_cdf.sh ${LAYOUT}_run0.dat

gnuplot << EOF
	set term pngcairo enhanced size 800,600 font 'Times new Roman, 16'
	set output "graph.png"
	set grid

	set key bottom right

	set xlabel "RTT Latency (ns)"
	set xrange [0:30000]

	set ylabel "CDF"
	set yrange [0:1.01]

	plot \
		'${LAYOUT}.cdf'			u 1:3 w l lw 4 t "${LAYOUT} (Real)", \
		'${LAYOUT}_run0.dat.cdf' 	u 1:3 w l lw 4 t "${LAYOUT} (Simulated)"
EOF
