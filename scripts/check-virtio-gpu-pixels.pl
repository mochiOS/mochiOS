#!/usr/bin/env perl
use strict;
use warnings;

sub fail {
    die "[error] virtio-gpu pixel check: $_[0]\n";
}

@ARGV == 1 or fail("usage: $0 SCREENSHOT.ppm");
my $path = $ARGV[0];
open my $fh, '<:raw', $path or fail("cannot open $path: $!");

sub read_token {
    my ($stream) = @_;
    my $token = '';
    while (read($stream, my $byte, 1) == 1) {
        if ($byte eq '#') {
            while (read($stream, $byte, 1) == 1 && $byte ne "\n") {}
            next;
        }
        next if $byte =~ /\s/;
        $token = $byte;
        last;
    }
    while (read($stream, my $byte, 1) == 1) {
        last if $byte =~ /\s/;
        $token .= $byte;
    }
    return $token;
}

read_token($fh) eq 'P6' or fail('screenshot is not a binary PPM');
my $width = read_token($fh);
my $height = read_token($fh);
my $maximum = read_token($fh);
$width =~ /^\d+$/ && $height =~ /^\d+$/ or fail('invalid dimensions');
$maximum eq '255' or fail("unsupported maximum component value: $maximum");
$width == 1280 && $height == 800
    or fail("unexpected scanout dimensions: ${width}x${height}");

my $expected_bytes = $width * $height * 3;
my $pixels = '';
read($fh, $pixels, $expected_bytes) == $expected_bytes
    or fail('truncated pixel data');
read($fh, my $extra, 1) == 0 or fail('trailing pixel data');

sub pixel_hex {
    my ($data, $stride, $x, $y) = @_;
    return unpack('H6', substr($data, ($y * $stride + $x) * 3, 3));
}

my @samples = (
    ['top-left background',      0,    0,   'c8c8c8'],
    ['top-right background',     1279, 0,   'c8c8c8'],
    ['bottom-left lower layer',  0,    799, 'c8c8c8,000000'],
    ['bottom-right background',  1279, 799, 'c8c8c8'],
    ['center window background', 640,  400, 'f7f7f7'],
    ['card foreground',          300,  375, 'ffffff'],
    ['accent border',            212,  300, '5f6fff'],
);

for my $sample (@samples) {
    my ($name, $x, $y, $expected_list) = @$sample;
    my $actual = pixel_hex($pixels, $width, $x, $y);
    my @expected = split /,/, $expected_list;
    grep { $actual eq $_ } @expected
        or fail(
            "$name at ($x,$y): expected "
            . join(' or ', map { "#$_" } @expected)
            . ", got #$actual"
        );
}

print "[check] virtio-gpu screenshot pixels verified\n";
