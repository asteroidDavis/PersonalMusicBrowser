from django.conf import settings
from django.db import models


class Instrument(models.Model):
    name = models.TextField(max_length="64", blank=False)

    def __str__(self):
        return self.name


class Band(models.Model):
    name = models.TextField(max_length=128, blank=False)

    def __str__(self):
        return self.name


class Artist(models.Model):
    name = models.TextField(max_length=128, blank=False)
    bands = models.ManyToManyField(Band)

    def __str__(self):
        return self.name


class Album(models.Model):
    title = models.TextField(max_length=256, blank=False)
    released = models.BooleanField(blank=False)
    url = models.URLField(blank=True)

    def __str__(self):
        return self.title


def get_discography_path() -> str:
    return settings.DISCOGRAPHY_ROOT


class Song(models.Model):
    title = models.TextField(max_length=256, blank=False)
    album = models.ForeignKey(Album, on_delete=models.PROTECT)
    artists = models.ManyToManyField(Artist)
    sheet_music = models.FilePathField(path=get_discography_path(), blank=True, allow_files=True, allow_folders=False,
                                       recursive=True)
    lyrics = models.FilePathField(path=get_discography_path(), blank=True, allow_files=True, allow_folders=False,
                                  recursive=True)

    def __str__(self):
        return f"{self.title} -- {self.album}"


class Cover(Song):
    notes = models.ImageField(blank=True)
    covered_instruments = models.ManyToManyField(Instrument)
    notes_completed = models.BooleanField()

    def __str__(self):
        return f"{super(self)} -- on {self.covered_instruments}"


class Composition(Song):
    written_instruments = models.ManyToManyField(Instrument)
    beats_per_minute_upper = models.IntegerField()
    beats_per_minute_lower = models.IntegerField()

    def __str__(self):
        return f"{super(self)} -- arranged for {self.written_instruments}"


class Recording(models.Model):
    instruments = models.ManyToManyField(Instrument)
    type = models.TextField(max_length=64, choices=[
        ('audacity',)*2, ('mix',)*2, ('master',)*2, ('loop-core-list',)*2, ('wav',)*2, ('audacity',)*2])
    path = models.FilePathField(path=get_discography_path(), blank=True, allow_files=True, allow_folders=False,
                                recursive=True)
    song = models.ForeignKey(Song, blank=False, on_delete=models.PROTECT)
    notes = models.ImageField(blank=True)

    def __str__(self):
        return f"{super(self)} -- in {self.type}"
