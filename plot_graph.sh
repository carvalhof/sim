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

	set xlabel "RTT Latency (us)"
	set xrange [0:30]

	set ylabel "CDF"
	set yrange [0:1.01]

	plot \
		'raw/${LAYOUT}.cdf'			u (column(1)/1000):3 w l lw 4 t "${LAYOUT} (Real)", \
		'${LAYOUT}_run0.dat.cdf' 	u (column(1)/1000):3 w l lw 4 t "${LAYOUT} (Simulated)"
EOF
