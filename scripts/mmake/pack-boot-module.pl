#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename qw(dirname);
use File::Temp qw(tempfile);

# The generated ext2 image reserves its first 1024 bytes for boot code. Use
# only the first 64 bytes for the generic mnu boot-module range descriptor;
# append the selected ELF without changing any filesystem block or pathname.
my ($operation, $image, $resource_bytes, $executable) = @ARGV;
die "usage: $0 check <image> <resource-bytes> | pack <image> <resource-bytes> <ELF>\n"
    unless defined($operation) && defined($resource_bytes)
        && $resource_bytes =~ /\A[1-9][0-9]*\z/
        && (($operation eq 'check' && @ARGV == 3)
            || ($operation eq 'pack' && @ARGV == 4));

open my $input, '<:raw', $image or die "open $image: $!\n";
my @image_stat = stat($input);
seek($input, 1024, 0) or die "seek $image: $!\n";
read($input, my $superblock, 60) == 60 or die "short ext2 superblock: $image\n";
my $blocks = unpack('V', substr($superblock, 4, 4));
my $log_block = unpack('V', substr($superblock, 24, 4));
my $magic = unpack('v', substr($superblock, 56, 2));
my $matches = $magic == 0xef53 && $log_block <= 6 && $blocks > 0
    && $blocks * (1024 << $log_block) == $resource_bytes
    && $image_stat[7] >= $resource_bytes;
exit($matches ? 0 : 1) if $operation eq 'check';
die "ext2 extent does not match resource size: $image\n" unless $matches;
seek($input, 0, 0) or die "seek $image: $!\n";
read($input, my $old_header, 64) == 64 or die "short boot header: $image\n";
die "refusing to replace unknown ext2 boot data: $image\n"
    unless $old_header eq "\0" x 64 || substr($old_header, 0, 8) eq "MNUBMOD\0";

open my $elf, '<:raw', $executable or die "open initial ELF $executable: $!\n";
my $image_bytes = -s $elf;
die "initial ELF must be nonempty and at most 64 MiB: $executable\n"
    unless defined($image_bytes) && $image_bytes >= 16 && $image_bytes <= 64 * 1024 * 1024;
read($elf, my $ident, 16) == 16 or die "short ELF identifier: $executable\n";
die "initial image must be a little-endian ELF64: $executable\n"
    unless substr($ident, 0, 7) eq "\x7fELF\x02\x01\x01";
seek($elf, 0, 0) or die "seek initial ELF: $!\n";

my ($output, $temporary) = tempfile('boot-module-XXXXXX', DIR => dirname($image), UNLINK => 1);
binmode($output);
my $header = pack('a8 V V Q< Q< Q<', "MNUBMOD\0", 1, 64,
    $resource_bytes, $resource_bytes, $image_bytes) . "\0" x 24;
print {$output} $header or die "write boot descriptor: $!\n";
sub copy_exact {
    my ($source, $destination, $remaining) = @_;
    while ($remaining > 0) {
        my $read = read($source, my $buffer, $remaining < 1048576 ? $remaining : 1048576);
        die "short boot-module input\n" unless defined($read) && $read > 0;
        print {$destination} $buffer or die "write boot-module data: $!\n";
        $remaining -= $read;
    }
}
copy_exact($input, $output, $resource_bytes - 64);
copy_exact($elf, $output, $image_bytes);
close($input) or die "close $image: $!\n";
close($elf) or die "close $executable: $!\n";
close($output) or die "close boot module: $!\n";
chmod($image_stat[2] & 0777, $temporary) == 1 or die "chmod boot module: $!\n";
rename($temporary, $image) or die "replace boot module $image: $!\n";
