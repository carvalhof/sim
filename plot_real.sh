#!/bin/bash

gnuplot << EOF
	set term pngcairo enhanced size 800,600 font 'Times new Roman, 16'
	set output "real.png"
	set grid

	set key top left

	set xlabel "RTT Latency (us)"
	set xrange [0:30]

	set ylabel "CDF"
	set yrange [0:1.01]

	plot \
		'raw/layout1.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 1", \
		'raw/layout2.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 2", \
		'raw/layout3.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 3", \
		'raw/layout4.cdf' 	u (column(1)/1000):3 w l lw 4 t "Layout 4"
EOF
