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

for my $line (@projects) {
    chomp $line;

    my ($project, $path, $remote, $revision) = split /\t/, $line, 4;

    next unless defined $project;
    next unless $project =~ m{^mochiOS/};

    $revision //= '';
    $revision =~ s{^refs/heads/}{};

    unless ($revision eq 'main' || $revision eq 'master') {
        print "[skip] $path: push先がmain/masterではありません: $revision\n";
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

    while (1) {
        print "\n";
        print "[$project]\n";
        print "  path:   $path\n";
        print "  push:   HEAD -> $remote/$revision\n";
        print "pushしますか？ [Y/n]: ";

        my $answer = <STDIN>;

        if (!defined $answer) {
            print "\n入力が終了したため中断します\n";
            exit 1;
        }

        chomp $answer;
        $answer =~ s/^\s+|\s+$//g;
        $answer = lc $answer;

        if ($answer eq '' || $answer eq 'y' || $answer eq 'yes') {
            print "[push] $path -> $remote/$revision\n";

            my $result = system(
                'git',
                '-C', $path,
                'push',
                $remote,
                "HEAD:refs/heads/$revision",
            );

            if ($result != 0) {
                print STDERR "[error] $path のpushに失敗しました\n";
                exit 1;
            }

            last;
        }

        if ($answer eq 'n' || $answer eq 'no') {
            print "[skip] $path\n";
            last;
        }

        print "y、n、または空欄で入力してください\n";
    }
}

print "\n[done] 処理が完了しました\n";

sub git_remote_exists {
    my ($path, $remote) = @_;

    return system(
        'git',
        '-C', $path,
        'remote',
        'get-url',
        $remote,
    ) == 0;
}