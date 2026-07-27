#!/usr/bin/env perl
use strict;
use warnings;

use IO::Socket::INET;

$| = 1;

@ARGV == 3 or die "usage: $0 <port> <ready-file> <once|persistent>\n";
my ($port, $ready_file, $mode) = @ARGV;
$port =~ /\A[0-9]+\z/ && $port > 0 && $port <= 65_535
    or die "invalid port: $port\n";
$mode eq 'once' || $mode eq 'persistent'
    or die "invalid mode: $mode\n";

my $server = IO::Socket::INET->new(
    LocalAddr => '127.0.0.1',
    LocalPort => $port,
    Proto => 'tcp',
    Listen => 1,
    ReuseAddr => 1,
) or die "network smoke server listen failed: $!\n";

open my $ready, '>', $ready_file or die "open $ready_file: $!\n";
print {$ready} "$port\n" or die "write $ready_file: $!\n";
close $ready or die "close $ready_file: $!\n";

while (1) {
    my $client = $server->accept() or die "network smoke server accept failed: $!\n";
    $client->autoflush(1);
    my $total = 0;
    while (1) {
        my $buffer = '';
        my $read = sysread($client, $buffer, 4096);
        defined $read or die "network smoke server read failed: $!\n";
        last if $read == 0;
        $total += $read;
        my $offset = 0;
        while ($offset < $read) {
            my $written = syswrite($client, $buffer, $read - $offset, $offset);
            defined $written && $written > 0
                or die "network smoke server write failed: $!\n";
            $offset += $written;
        }
    }
    close $client or die "network smoke server client close failed: $!\n";
    print "network-smoke-server: echoed=$total\n";
    last if $mode eq 'once';
}
close $server or die "network smoke server close failed: $!\n";
