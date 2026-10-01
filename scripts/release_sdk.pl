#!/usr/bin/env perl

use strict;
use warnings;

use Cwd qw(abs_path);
use Digest::SHA ();
use File::Basename qw(basename dirname);
use File::Copy qw(copy);
use File::Find qw(find);
use File::Path qw(make_path remove_tree);
use File::Spec;
use File::Temp qw(tempfile);
use Getopt::Long qw(GetOptions);

sub usage {
    my ($status) = @_;
    my $fh = $status == 0 ? *STDOUT : *STDERR;
    print {$fh} "usage: $0 [--input SDK] [--output DIR] [--version-file FILE] [--sdk-version VERSION] [--architecture ARCH] [--compiler-runtime NAME] [--archive FILE]\n";
    exit $status;
}

sub absolute_path {
    my ($path) = @_;
    return File::Spec->canonpath(File::Spec->rel2abs($path));
}

sub is_within {
    my ($path, $directory) = @_;
    return $path eq $directory || index("$path/", "$directory/") == 0;
}

sub require_file {
    my ($root, $relative) = @_;
    my $path = File::Spec->catfile($root, split m{/}, $relative);
    -f $path && -r $path && -s $path
        or die "required SDK file is missing or empty: $path\n";
}

sub require_directory {
    my ($root, $relative) = @_;
    my $path = File::Spec->catdir($root, split m{/}, $relative);
    -d $path && -r $path
        or die "required SDK directory is missing: $path\n";
}

sub copy_file {
    my ($source, $destination) = @_;
    -l $source and die "SDK release does not permit symlinks: $source\n";
    -f $source or die "SDK release expected a regular file: $source\n";
    make_path(dirname($destination));
    copy($source, $destination) or die "copy $source to $destination: $!\n";
    my $mode = (stat($source))[2] & 07777;
    chmod $mode, $destination or die "chmod $destination: $!\n";
}

sub copy_tree {
    my ($source, $destination) = @_;
    make_path($destination);
    find(
        {
            no_chdir => 1,
            wanted => sub {
                my $path = $File::Find::name;
                return if $path eq $source;
                my $relative = File::Spec->abs2rel($path, $source);
                my $target = File::Spec->catfile($destination, $relative);
                -l $path and die "SDK release does not permit symlinks: $path\n";
                if (-d $path) {
                    make_path($target);
                    return;
                }
                -f $path or die "SDK release found a non-regular entry: $path\n";
                copy_file($path, $target);
            },
        },
        $source,
    );
}

sub run_command {
    my (@command) = @_;
    system(@command) == 0 or die "command failed: @command\n";
}

sub read_sdk_version {
    my ($path) = @_;
    open(my $fh, '<', $path) or die "open version file $path: $!\n";
    my $version;
    while (my $line = <$fh>) {
        if ($line =~ /^\s*release\s*=\s*"([^"]+)"\s*(?:#.*)?$/) {
            $version = $1;
            last;
        }
    }
    close($fh) or die "close version file $path: $!\n";
    defined $version && length $version
        or die "release version is missing from $path\n";
    return $version;
}

my $script = abs_path($0) or die "resolve script path: $!\n";
my $root = abs_path(File::Spec->catdir(dirname($script), '..'))
    or die "resolve repository root: $!\n";
my $input = File::Spec->catdir($root, 'out', 'newlib-port', 'sdk');
my $output = File::Spec->catdir($root, 'out', 'release', 'sdk');
my $version_file = File::Spec->catfile($root, 'version.toml');
my $sdk_version;
my $architecture = 'x86_64';
my $compiler_runtime = 'libgcc';
my $archive;

GetOptions(
    'input=s'            => \$input,
    'output=s'           => \$output,
    'version-file=s'     => \$version_file,
    'sdk-version=s'      => \$sdk_version,
    'architecture=s'     => \$architecture,
    'compiler-runtime=s' => \$compiler_runtime,
    'archive=s'          => \$archive,
    'help'               => sub { usage(0) },
) or usage(2);
@ARGV == 0 or usage(2);

$input = absolute_path($input);
$output = absolute_path($output);
$version_file = absolute_path($version_file);
-d $input or die "built SDK input directory is missing: $input\n";
-f $version_file or die "version file is missing: $version_file\n";
$architecture =~ /^[A-Za-z0-9_.-]+$/ or die "invalid SDK architecture: $architecture\n";
$sdk_version //= read_sdk_version($version_file);
$sdk_version =~ /^[A-Za-z0-9_.+-]+$/ or die "invalid SDK version: $sdk_version\n";

my %runtime_layout = (
    libgcc => {
        source      => 'lib/libgcc.a',
        destination => 'lib/libgcc.a',
    },
    'compiler-rt' => {
        source      => 'lib/libclang_rt.builtins-x86_64.a',
        destination => 'lib/libclang_rt.builtins-x86_64.a',
    },
);
exists $runtime_layout{$compiler_runtime}
    or die "unsupported compiler runtime '$compiler_runtime' (expected libgcc or compiler-rt)\n";

my @required_files = (
    'lib/crt0.o',
    'lib/linker.ld',
    'lib/libmochi_user_newlib_runtime.a',
    'sysroot/lib/libc.a',
    'sysroot/lib/libm.a',
    $runtime_layout{$compiler_runtime}->{source},
);
require_file($input, $_) for @required_files;
require_directory($input, 'sysroot/include');

is_within($input, $output)
    and die "refusing to clean output directory containing the input SDK: $output\n";
is_within($root, $output)
    and die "refusing to use repository root or its parent as output: $output\n";
$output eq File::Spec->rootdir()
    and die "refusing to use filesystem root as output\n";
-l $output and die "refusing to clean symlink output directory: $output\n";

if (-e $output) {
    remove_tree($output);
    !-e $output or die "failed to clean release SDK output: $output\n";
}
make_path($output);

for my $relative (@required_files) {
    my $destination = $relative eq $runtime_layout{$compiler_runtime}->{source}
        ? $runtime_layout{$compiler_runtime}->{destination}
        : $relative;
    copy_file(
        File::Spec->catfile($input, split m{/}, $relative),
        File::Spec->catfile($output, split m{/}, $destination),
    );
}
copy_tree(
    File::Spec->catdir($input, 'sysroot', 'include'),
    File::Spec->catdir($output, 'sysroot', 'include'),
);

for my $relative ('bin/mochios-cc', 'share/x86_64-unknown-mochios.json') {
    my $source = File::Spec->catfile($input, split m{/}, $relative);
    next unless -e $source;
    copy_file($source, File::Spec->catfile($output, split m{/}, $relative));
}

my $manifest = File::Spec->catfile($output, 'manifest.toml');
open(my $manifest_fh, '>', $manifest) or die "create $manifest: $!\n";
print {$manifest_fh} <<"MANIFEST";
format = 1
sdk_version = "$sdk_version"
architecture = "$architecture"
target = "x86_64-unknown-mochios"
compiler_runtime = "$compiler_runtime"
MANIFEST
close($manifest_fh) or die "close $manifest: $!\n";
chmod 0644, $manifest or die "chmod $manifest: $!\n";

my $archive_name = "mochios-sdk-$sdk_version-$architecture.tar.zst";
$archive //= File::Spec->catfile(dirname($output), $archive_name);
$archive = absolute_path($archive);
my $checksum = "$archive.sha256";
make_path(dirname($archive));
unlink($archive) if -e $archive;
unlink($checksum) if -e $checksum;

my ($tar_fh, $tar_path) = tempfile('mochios-sdk-XXXXXX', DIR => dirname($archive), UNLINK => 1);
close($tar_fh) or die "close temporary tar file: $!\n";
my $source_date_epoch = $ENV{SOURCE_DATE_EPOCH} // 0;
$source_date_epoch =~ /^\d+$/ or die "SOURCE_DATE_EPOCH must be an unsigned integer\n";
run_command(
    'tar',
    '--sort=name',
    '--format=ustar',
    '--owner=0',
    '--group=0',
    '--numeric-owner',
    "--mtime=\@$source_date_epoch",
    '-C', dirname($output),
    '-cf', $tar_path,
    basename($output),
);
run_command('zstd', '-q', '-f', '-19', $tar_path, '-o', $archive);

open(my $archive_fh, '<:raw', $archive) or die "open $archive: $!\n";
my $sha = Digest::SHA->new(256);
$sha->addfile($archive_fh);
close($archive_fh) or die "close $archive: $!\n";
my $digest = $sha->hexdigest;
open(my $checksum_fh, '>', $checksum) or die "create $checksum: $!\n";
print {$checksum_fh} "$digest  ", basename($archive), "\n";
close($checksum_fh) or die "close $checksum: $!\n";

print "[done] release SDK: $output\n";
print "[done] archive: $archive\n";
print "[done] SHA-256: $checksum\n";
