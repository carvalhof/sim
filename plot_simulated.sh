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

	set xlabel "RTT Latency (us)"
	set xrange [0:30]

	set ylabel "CDF"
	set yrange [0:1.01]

	plot \
		'layout1_run0.dat.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 1", \
		'layout2_run0.dat.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 2", \
		'layout3_run0.dat.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 3", \
		'layout4_run0.dat.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 4"
EOF
