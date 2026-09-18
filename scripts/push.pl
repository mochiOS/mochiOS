#!/usr/bin/env perl

use strict;
use warnings;
use utf8;

binmode STDIN, ':encoding(UTF-8)';
binmode STDOUT, ':encoding(UTF-8)';
binmode STDERR, ':encoding(UTF-8)';

if (!-d '.repo') {
    die "error: repoワークスペースのルートで実行してください\n";
}

my @projects = `repo forall -c 'printf "%s\\t%s\\t%s\\t%s\\n" "\$REPO_PROJECT" "\$REPO_PATH" "\$REPO_REMOTE" "\$REPO_RREV"'`;

if ($? != 0) {
    die "error: repoからプロジェクト情報を取得できませんでした\n";
}

my @targets;
my $preflight_failed = 0;
for my $line (@projects) {
    chomp $line;

    my ($project, $path, $remote, $revision) = split /\t/, $line, 4;

    next unless defined $project;
    next unless $project =~ m{^mochiOS/};

    $revision //= '';
    $revision =~ s{^refs/heads/}{};
    if ($revision eq '') {
        print STDERR "[error] $path: push先branchを特定できません\n";
        next;
    }

    $remote ||= 'github';

    unless (git_remote_exists($path, $remote)) {
        if (git_remote_exists($path, 'origin')) {
            $remote = 'origin';
        }
        else {
            print STDERR "[error] $path: 利用可能なremoteがありません\n";
            next;
        }
    }

    unless (git_fetch_branch($path, $remote, $revision)) {
        print STDERR "[error] $path: $remote/$revision の取得に失敗しました\n";
        $preflight_failed = 1;
        next;
    }

    my ($ahead, $behind) = git_ahead_behind($path, $remote, $revision);
    unless (defined $ahead && defined $behind) {
        print STDERR "[error] $path: $remote/$revision との差分を確認できません\n";
        $preflight_failed = 1;
        next;
    }

    if ($behind > 0) {
        my $state = $ahead > 0 ? '分岐しています' : 'pullが必要です';
        print STDERR "[pull required] $path: $remote/$revision より $behind commit遅れており、$state\n";
        $preflight_failed = 1;
        next;
    }

    next unless $ahead > 0;
    push @targets, {
        project  => $project,
        path     => $path,
        remote   => $remote,
        revision => $revision,
    };
}

if ($preflight_failed) {
    die "error: remote側の変更を取り込んでから再実行してください\n";
}

if (!@targets) {
    print "push対象のリポジトリはありません\n";
    exit 0;
}

print "push対象のリポジトリ:\n";
for my $target (@targets) {
    print "  $target->{project} ($target->{path}) -> $target->{remote}/$target->{revision}\n";
}

while (1) {
    print "\n上記", scalar(@targets), "リポジトリをpushしますか？ [Y/n]: ";

    my $answer = <STDIN>;
    if (!defined $answer) {
        print "\n入力が終了したため中断します\n";
        exit 1;
    }

    chomp $answer;
    $answer =~ s/^\s+|\s+$//g;
    $answer = lc $answer;

    if ($answer eq 'n' || $answer eq 'no') {
        print "[cancel] pushを中断しました\n";
        exit 0;
    }
    last if $answer eq '' || $answer eq 'y' || $answer eq 'yes';
    print "y、n、または空欄で入力してください\n";
}

for my $target (@targets) {
    print "[push] $target->{path} -> $target->{remote}/$target->{revision}\n";

    my $result = system(
        'git',
        '-C', $target->{path},
        'push',
        $target->{remote},
        "HEAD:refs/heads/$target->{revision}",
    );

    if ($result != 0) {
        print STDERR "[error] $target->{path} のpushに失敗しました\n";
        exit 1;
    }
}

print "\n[done] 処理が完了しました\n";

sub git_remote_exists {
    my ($path, $remote) = @_;
    my $remotes = git_output('git', '-C', $path, 'remote');
    return 0 unless defined $remotes;
    return scalar grep { $_ eq $remote } split /\n/, $remotes;
}

sub git_fetch_branch {
    my ($path, $remote, $revision) = @_;
    my $refspec = "+refs/heads/$revision:refs/remotes/$remote/$revision";
    return system(
        'git', '-C', $path,
        'fetch', '--quiet', $remote, $refspec,
    ) == 0;
}

sub git_ahead_behind {
    my ($path, $remote, $revision) = @_;
    my $counts = git_output(
        'git', '-C', $path,
        'rev-list', '--left-right', '--count',
        "HEAD...refs/remotes/$remote/$revision",
    );
    return unless defined $counts;
    return unless $counts =~ /^([0-9]+)\s+([0-9]+)$/;
    return ($1 + 0, $2 + 0);
}

sub git_output {
    my (@command) = @_;
    open my $fh, '-|', @command or return undef;
    local $/;
    my $output = <$fh>;
    close $fh or return undef;
    $output //= '';
    $output =~ s/\s+\z//;
    return $output;
}
