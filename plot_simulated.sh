#!/bin/bash

./script_cdf.sh layout1_run0.dat
./script_cdf.sh layout2_run0.dat
./script_cdf.sh layout3_run0.dat
./script_cdf.sh layout4_run0.dat

gnuplot << EOF
	set term pngcairo enhanced size 800,600 font 'Times new Roman, 16'
	set output "simulation.png"
	set grid

	set key bottom right

	set xlabel "RTT Latency (ns)"
	set xrange [0:30000]

	set ylabel "CDF"
	set yrange [0:1.01]

	plot \
		'layout1_run0.dat.cdf' 	u 1:3 w l lw 4 t "layout 1 (Simulated)", \
		'layout2_run0.dat.cdf' 	u 1:3 w l lw 4 t "layout 2 (Simulated)", \
		'layout3_run0.dat.cdf' 	u 1:3 w l lw 4 t "layout 3 (Simulated)", \
		'layout4_run0.dat.cdf' 	u 1:3 w l lw 4 t "layout 4 (Simulated)"
EOF
